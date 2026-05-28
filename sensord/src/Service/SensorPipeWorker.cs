using System.IO.Pipes;
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
    private readonly IHostApplicationLifetime _lifetime;
    private readonly string _pipeName;
    private int _intervalMs = 1000;
    private HealthInfo? _health;

    public SensorPipeWorker(ILogger<SensorPipeWorker> log, IHostApplicationLifetime lifetime, string pipeName)
    {
        _log = log;
        _lifetime = lifetime;
        _pipeName = pipeName;
    }

    /// <summary>
    /// Mutable holder so the async <see cref="ServeOne"/> can reassign the
    /// underlying <see cref="HardwareMonitor"/> on topology change without
    /// needing a ref-parameter (forbidden on async methods). The
    /// <c>finally</c> in <see cref="ExecuteAsync"/> disposes the current
    /// monitor through this reference, so a swap during the session is
    /// always reflected at shutdown.
    /// </summary>
    private sealed class MonitorRef
    {
        public HardwareMonitor Monitor;
        public MonitorRef(HardwareMonitor m) { Monitor = m; }
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        _log.LogInformation("SensorPipeWorker started, pipe={Pipe}", _pipeName);

        // Order matters for startup latency. The dashboard polls the pipe in
        // 250 ms ticks with a short deadline; if any of the steps below ran
        // before pipe creation, a slow HardwareMonitor.Open (several seconds
        // on first launch after a PawnIO install) would blow that budget and
        // the dashboard would surface its startup-timeout error. So:
        //   1) Create the pipe immediately — dashboard connects within ms.
        //   2) Wait for the client (30 s; longer than the dashboard's wait so
        //      the dashboard always gives up first).
        //   3) THEN do the expensive HardwareMonitor init / probe / disk scan,
        //      with the client already attached.
        // Every step is wrapped: an uncaught throw before the first `await`
        // would surface as a synchronous SCM failure with no log breadcrumbs.
        NamedPipeServerStream? pipe = null;
        MonitorRef? monitorRef = null;
        try
        {
            try
            {
                pipe = PipeServer.Create(_pipeName);
            }
            catch (Exception ex)
            {
                _log.LogError(ex, "pipe creation failed, stopping service");
                return;
            }
            _log.LogInformation("Waiting for client on {Pipe}", _pipeName);

            using (var acceptCts = CancellationTokenSource.CreateLinkedTokenSource(stoppingToken))
            {
                acceptCts.CancelAfter(TimeSpan.FromSeconds(30));
                try
                {
                    await pipe.WaitForConnectionAsync(acceptCts.Token);
                }
                catch (OperationCanceledException) when (!stoppingToken.IsCancellationRequested)
                {
                    _log.LogInformation("No client connected within 30 s, stopping service");
                    return;
                }
                catch (IOException ex)
                {
                    _log.LogError(ex, "pipe accept failed, stopping service");
                    return;
                }
            }
            _log.LogInformation("Client connected");

            // From here on the client is attached. If HardwareMonitor.Open
            // throws (broken PawnIO driver, AV quarantine on the kernel
            // driver binary, SCM-side CreateService failure), surface a
            // degraded health snapshot so the dashboard's health banner
            // explains the problem instead of just disconnecting with no
            // signal. The `when` filter on the catch lets a host-issued
            // cancellation propagate as normal shutdown rather than be
            // mis-classified as a PawnIO denial.
            HardwareMonitor monitor;
            try
            {
                monitor = new HardwareMonitor();
            }
            catch (Exception ex) when (ex is not OperationCanceledException)
            {
                _log.LogError(ex, "HardwareMonitor.Open failed");
                await TryWriteFatalHealth(pipe,
                    pawnio: "denied",
                    notes: $"HardwareMonitor init failed: {ex.Message}");
                return;
            }
            monitorRef = new MonitorRef(monitor);

            var diskInfo = DiskInfoReader.Read();
            _health = RunProbe(monitorRef.Monitor);
            _log.LogInformation("PawnIO probe: {Pawnio} (degraded={Degraded})",
                _health.Pawnio, _health.Degraded);

            await ServeOne(pipe, monitorRef, diskInfo, stoppingToken);
            _log.LogInformation("Client disconnected, stopping service");
        }
        finally
        {
            monitorRef?.Monitor.Dispose();
            if (pipe is not null)
                await pipe.DisposeAsync();
            // BackgroundService.ExecuteAsync completing normally does NOT stop
            // the host in .NET 8 — the process would idle forever with no
            // hosted work. Explicitly request shutdown so the service exits
            // as soon as the worker returns (client disconnect, shutdown
            // message, or 30 s startup timeout).
            _lifetime.StopApplication();
        }
    }

    /// <summary>
    /// Health-only snapshot — exists because a silent exit on
    /// HardwareMonitor.Open failure leaves the dashboard with no signal
    /// beyond a pipe disconnect, and "the service started and then
    /// disappeared" is one of the hardest crash classes to triage.
    /// Best-effort: a client that has already detached, a JSON serializer
    /// fault or a kernel write error must not become a second crash on top
    /// of the original one, so the catch swallows — but it does emit one
    /// warning so the Event Log explains why no banner reached the user.
    /// </summary>
    private async Task TryWriteFatalHealth(Stream pipe, string pawnio, string notes)
    {
        try
        {
            await using var writer = new StreamWriter(
                pipe, new UTF8Encoding(false), bufferSize: 8192, leaveOpen: true)
            {
                AutoFlush = true,
                NewLine = "\n"
            };
            var snap = new Snapshot(
                Version: 1,
                Timestamp: DateTimeOffset.UtcNow.ToUnixTimeSeconds(),
                Cpu: null, Gpu: null, Igpu: null, Ram: null, Storage: null,
                Board: null, Fans: null, Voltages: null, Net: null, Battery: null,
                UptimeSec: null, AtkFans: null, Display: null, Displays: null,
                Health: new HealthInfo(pawnio, Degraded: true, Notes: notes));
            string json = JsonSerializer.Serialize(snap, SensordJsonContext.Default.Snapshot);
            await writer.WriteLineAsync(json);
            // Force the bytes onto the kernel pipe buffer before the
            // finally block disposes the pipe — otherwise the writer's
            // async-flush on dispose races with pipe teardown.
            await pipe.FlushAsync();
        }
        catch (Exception ex)
        {
            _log.LogWarning(ex, "fatal-health snapshot not delivered to client");
        }
    }

    private async Task ServeOne(
        Stream pipe,
        MonitorRef monitorRef,
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
        // We don't await it. When ServeOne returns, ExecuteAsync's finally
        // block disposes the pipe and ReadLine() returns null, ending the
        // loop. The thread-pool thread is then released. One-shot session
        // lifetime means no cross-session leak.
        _ = Task.Run(
            () => ReadControlLoop(reader, clientCts.Token, () => clientCts.Cancel()),
            clientCts.Token);

        int tickCounter = 0;
        HashSet<string> currentIds = TopologySignature.IdentifierSet(monitorRef.Monitor.Refresh());
        var currentDiskInfo = diskInfo;

        while (!clientCts.Token.IsCancellationRequested)
        {
            try
            {
                var hardware = monitorRef.Monitor.Refresh();

                if (++tickCounter >= 5)
                {
                    tickCounter = 0;
                    var newIds = TopologySignature.IdentifierSet(hardware);
                    if (!newIds.SetEquals(currentIds))
                    {
                        var added = string.Join(",", newIds.Except(currentIds));
                        var removed = string.Join(",", currentIds.Except(newIds));
                        _log.LogInformation(
                            "Topology change detected (added=[{Added}] removed=[{Removed}]); recreating monitor",
                            added, removed);

                        // Construct first so a Computer.Open() failure leaves the old (working)
                        // monitor in place; the next 5-tick window will retry the topology check.
                        var newMonitor = new HardwareMonitor();
                        var oldMonitor = monitorRef.Monitor;
                        monitorRef.Monitor = newMonitor;
                        oldMonitor.Dispose();
                        currentDiskInfo = DiskInfoReader.Read();
                        hardware = monitorRef.Monitor.Refresh();
                        currentIds = TopologySignature.IdentifierSet(hardware);
                    }
                }

                Snapshot snap = SnapshotBuilder.Build(
                    hardware, PagefileReader.Read(), LinkSpeeds(), currentDiskInfo)
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
