using System.Net.NetworkInformation;
using System.Text;
using System.Text.Json;
using LibreHardwareMonitor.Hardware;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using Sensord.Control;
using Sensord.Model;
using Sensord.Sensors;

namespace Sensord.Service;

/// <summary>
/// Long-running service loop. Accepts one client at a time, emits NDJSON
/// snapshots once per tick on the downstream half, and parses control
/// messages off the upstream half. Tears the pipe down + reopens on client
/// disconnect. The poll loop is suspended while no client is connected.
/// </summary>
internal sealed class SensorPipeWorker : BackgroundService
{
    private readonly ILogger<SensorPipeWorker> _log;
    private readonly string _pipeName;
    private int _intervalMs = 1000;
    private HealthInfo? _health;

    public SensorPipeWorker(ILogger<SensorPipeWorker> log, string pipeName)
    {
        _log = log;
        _pipeName = pipeName;
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        _log.LogInformation("SensorPipeWorker started, pipe={Pipe}", _pipeName);

        var diskInfo = DiskInfoReader.Read();
        using var monitor = new HardwareMonitor();
        _health = RunProbe(monitor);
        _log.LogInformation("PawnIO probe: {Pawnio} (degraded={Degraded})",
            _health.Pawnio, _health.Degraded);

        while (!stoppingToken.IsCancellationRequested)
        {
            try
            {
                await using var pipe = PipeServer.Create(_pipeName);
                _log.LogInformation("Waiting for client on {Pipe}", _pipeName);
                await pipe.WaitForConnectionAsync(stoppingToken);
                _log.LogInformation("Client connected");
                await ServeOne(pipe, monitor, diskInfo, stoppingToken);
                _log.LogInformation("Client disconnected, reopening pipe");
            }
            catch (OperationCanceledException)
            {
                break;
            }
            catch (Exception ex)
            {
                _log.LogError(ex, "pipe loop error");
                try { await Task.Delay(1000, stoppingToken); }
                catch (OperationCanceledException) { break; }
            }
        }
    }

    private async Task ServeOne(
        Stream pipe,
        HardwareMonitor monitor,
        IReadOnlyDictionary<int, PhysicalDiskInfo> diskInfo,
        CancellationToken token)
    {
        using var writer = new StreamWriter(
            pipe, new UTF8Encoding(false), bufferSize: 8192, leaveOpen: true)
        {
            AutoFlush = true,
            NewLine = "\n"
        };
        // Reader runs in parallel for upstream control messages.
        var reader = new StreamReader(
            pipe, Encoding.UTF8, detectEncodingFromByteOrderMarks: false,
            bufferSize: 1024, leaveOpen: true);
        _ = Task.Run(() => ReadControlLoop(reader, token), token);

        while (!token.IsCancellationRequested)
        {
            try
            {
                var hardware = monitor.Refresh();
                Snapshot snap = SnapshotBuilder.Build(
                    hardware, PagefileReader.Read(), LinkSpeeds(), diskInfo)
                    with { Health = _health };
                string json = JsonSerializer.Serialize(snap, SensordJsonContext.Default.Snapshot);
                await writer.WriteLineAsync(json);
                await Task.Delay(_intervalMs, token);
            }
            catch (IOException)
            {
                return; // client gone
            }
            catch (OperationCanceledException)
            {
                return;
            }
            catch (Exception ex)
            {
                _log.LogError(ex, "snapshot emit error");
                try { await Task.Delay(1000, token); }
                catch (OperationCanceledException) { return; }
            }
        }
    }

    private void ReadControlLoop(StreamReader reader, CancellationToken token)
    {
        try
        {
            string? line;
            while (!token.IsCancellationRequested && (line = reader.ReadLine()) is not null)
            {
                ControlMessage? msg = ControlReader.Parse(line);
                if (msg?.IntervalMs is int ms && ms is >= 250 and <= 60_000)
                {
                    Interlocked.Exchange(ref _intervalMs, ms);
                    _log.LogInformation("Interval changed to {Ms} ms", ms);
                }
            }
        }
        catch (IOException) { /* client gone */ }
    }

    private static Dictionary<string, long> LinkSpeeds()
    {
        var map = new Dictionary<string, long>();
        foreach (var ni in NetworkInterface.GetAllNetworkInterfaces())
            if (ni.OperationalStatus == OperationalStatus.Up)
                map[ni.Name] = ni.Speed;
        return map;
    }

    private static HealthInfo RunProbe(HardwareMonitor monitor)
    {
        try
        {
            var hw = monitor.Refresh();
            var temps = new List<double?>();
            foreach (var h in hw)
                foreach (var s in h.Sensors)
                    if (s.SensorType == SensorType.Temperature)
                        temps.Add(s.Value.HasValue ? (double?)s.Value.Value : null);
            return HealthProbe.Classify(temps, exception: null);
        }
        catch (Exception ex)
        {
            return HealthProbe.Classify(temps: null, exception: ex);
        }
    }
}
