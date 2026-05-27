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
/// Per-session worker. Accepts at most one client per service start, emits
/// NDJSON snapshots on the downstream half while the client is connected,
/// parses control messages off the upstream half, and exits when the
/// client disconnects, sends <c>{"shutdown":true}</c>, or 30 s pass
/// without a client.
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
        // Plain `var` + try/finally below: Task 2 of the v0.9.0 plan reassigns
        // `monitor` on topology change, which `using` would forbid. Keeping the
        // `try/finally` shape now to avoid churn when that lands.
        var monitor = new HardwareMonitor();
        _health = RunProbe(monitor);
        _log.LogInformation("PawnIO probe: {Pawnio} (degraded={Degraded})",
            _health.Pawnio, _health.Degraded);

        try
        {
            await using var pipe = PipeServer.Create(_pipeName);
            _log.LogInformation("Waiting for client on {Pipe}", _pipeName);

            using var acceptCts = CancellationTokenSource.CreateLinkedTokenSource(stoppingToken);
            acceptCts.CancelAfter(TimeSpan.FromSeconds(30));
            try
            {
                await pipe.WaitForConnectionAsync(acceptCts.Token);
            }
            catch (OperationCanceledException) when (!stoppingToken.IsCancellationRequested)
            {
                _log.LogInformation("No client connected within 30s, stopping service");
                return;
            }
            catch (IOException ex)
            {
                _log.LogError(ex, "pipe accept failed, stopping service");
                return;
            }

            _log.LogInformation("Client connected");
            await ServeOne(pipe, monitor, diskInfo, stoppingToken);
            _log.LogInformation("Client disconnected, stopping service");
        }
        finally
        {
            monitor.Dispose();
        }
    }

    private async Task ServeOne(
        Stream pipe,
        HardwareMonitor monitor,
        IReadOnlyDictionary<int, PhysicalDiskInfo> diskInfo,
        CancellationToken token)
    {
        using var clientCts = CancellationTokenSource.CreateLinkedTokenSource(token);
        using var writer = new StreamWriter(
            pipe, new UTF8Encoding(false), bufferSize: 8192, leaveOpen: true)
        {
            AutoFlush = true,
            NewLine = "\n"
        };
        var reader = new StreamReader(
            pipe, Encoding.UTF8, detectEncodingFromByteOrderMarks: false,
            bufferSize: 1024, leaveOpen: true);
        // Fire-and-forget: the reader blocks in ReadLine() on the pipe handle.
        // We don't await it. When ServeOne returns, `await using var pipe` in
        // ExecuteAsync disposes the pipe and ReadLine() returns null, ending the
        // loop. The thread-pool thread is then released. One-shot session
        // lifetime means no cross-session leak.
        _ = Task.Run(
            () => ReadControlLoop(reader, clientCts.Token, () => clientCts.Cancel()),
            clientCts.Token);

        while (!clientCts.Token.IsCancellationRequested)
        {
            try
            {
                var hardware = monitor.Refresh();
                Snapshot snap = SnapshotBuilder.Build(
                    hardware, PagefileReader.Read(), LinkSpeeds(), diskInfo)
                    with
                { Health = _health };
                string json = JsonSerializer.Serialize(snap, SensordJsonContext.Default.Snapshot);
                await writer.WriteLineAsync(json);
                await Task.Delay(_intervalMs, clientCts.Token);
            }
            catch (IOException) { return; }
            catch (OperationCanceledException)
            {
                // Task.Delay observed clientCts.Cancel(); shutdown requested.
                return;
            }
            catch (Exception ex)
            {
                _log.LogError(ex, "snapshot emit error");
                try { await Task.Delay(1000, clientCts.Token); }
                catch (OperationCanceledException) { return; }
            }
        }
    }

    private void ReadControlLoop(StreamReader reader, CancellationToken token, Action onShutdownRequested)
    {
        try
        {
            string? line;
            while (!token.IsCancellationRequested && (line = reader.ReadLine()) is not null)
            {
                ControlMessage? msg = ControlReader.Parse(line);
                if (msg is null) continue;

                if (msg.Shutdown)
                {
                    _log.LogInformation("Shutdown requested by client");
                    onShutdownRequested();
                    return;
                }
                if (msg.IntervalMs is int ms && ms is >= 250 and <= 60_000)
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
