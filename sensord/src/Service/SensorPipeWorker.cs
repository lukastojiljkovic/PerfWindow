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
/// parses control messages off the upstream half, and exits when the client
/// disconnects, sends <c>{"shutdown":true}</c>, or 30 s pass without a
/// client. Sensor init is staged: CPU + RAM come up first (first snapshot in
/// ~1-2 s), the remaining LHM categories are enabled one per serve-loop
/// iteration with progress lines in between (v0.10 protocol contract).
/// Once a session ends, process exit is bounded by
/// <see cref="TeardownGrace"/> — see <see cref="ArmTeardownWatchdog"/>.
/// </summary>
internal sealed class SensorPipeWorker : BackgroundService
{
    private readonly ILogger<SensorPipeWorker> _log;
    private readonly IHostApplicationLifetime _lifetime;
    private readonly string _pipeName;
    private int _intervalMs = 1000;
    private HealthInfo? _health;
    private int _teardownWatchdogArmed;

    /// <summary>
    /// Hard deadline on teardown once the session has ended. Long enough for
    /// a normal wind-down (Computer.Close + pipe disposal + host stop, ~2 s
    /// worst case), short enough that a client that just closed the dashboard
    /// never has a lingering LocalSystem process behind it.
    /// </summary>
    private static readonly TimeSpan TeardownGrace = TimeSpan.FromSeconds(3.5);

    /// <summary>
    /// Set when the worker ended because startup itself failed (pipe
    /// creation, sensor init, worker crash). Program maps it to a non-zero
    /// process exit code so a broken install never reads as a clean stop
    /// (SND-10).
    /// </summary>
    public bool FatalInit { get; private set; }

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

        // Order matters both for startup latency and for control
        // responsiveness:
        //   1) Create the pipe immediately — the dashboard connects within ms.
        //   2) Wait for the client (30 s; longer than the dashboard's wait so
        //      the dashboard always gives up first).
        //   3) Start the control reader BEFORE any LibreHardwareMonitor work:
        //      a {"shutdown":true} or client EOF during the multi-second
        //      sensor init must end the session within ~1 s, not queue behind
        //      it.
        //   4) Stage the expensive init — WMI disk scan in parallel with a
        //      CPU+RAM-only Computer.Open; the remaining categories are
        //      enabled from the serve loop with progress lines in between.
        // The holders are nullable because the finally must clean up whatever
        // was actually built — possibly nothing, if pipe creation or the
        // client-accept step failed first.
        NamedPipeServerStream? pipe = null;
        MonitorRef? monitorRef = null;
        StreamWriter? writer = null;
        CancellationTokenSource? clientCts = null;
        Task? controlTask = null;
        try
        {
            try
            {
                pipe = PipeServer.Create(_pipeName);
            }
            catch (Exception ex)
            {
                _log.LogError(ex, "pipe creation failed, stopping service");
                FatalInit = true;
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
                    FatalInit = true;
                    return;
                }
            }
            _log.LogInformation("Client connected");

            clientCts = CancellationTokenSource.CreateLinkedTokenSource(stoppingToken);
            CancellationToken sessionCt = clientCts.Token;
            CancellationTokenSource sessionCts = clientCts;
            void RequestSessionEnd()
            {
                // The control task can outlive this method's scope; Cancel on
                // a disposed CTS throws and would become an unobserved fault.
                try { sessionCts.Cancel(); }
                catch (ObjectDisposedException) { }
                // The serve loop may be inside a synchronous LHM call
                // (Refresh / EnableNextCategory) that cannot observe the
                // cancellation for seconds — bound the wind-down from here.
                ArmTeardownWatchdog();
            }

            // No `using`: StreamWriter.Dispose() synchronously flushes, and
            // flushing a closed pipe throws `IOException: Pipe is broken` on
            // every normal client disconnect. Disposed manually in the
            // finally with that flush failure swallowed.
            writer = new StreamWriter(
                pipe, new UTF8Encoding(false), bufferSize: 8192, leaveOpen: true)
            {
                AutoFlush = true,
                NewLine = "\n"
            };
            var reader = new StreamReader(
                pipe, Encoding.UTF8, detectEncodingFromByteOrderMarks: false,
                bufferSize: 1024, leaveOpen: true);
            controlTask = Task.Run(
                () => ReadControlLoop(reader, sessionCt, RequestSessionEnd),
                CancellationToken.None);

            IReadOnlyList<IHardware> firstHardware;
            IReadOnlyDictionary<int, PhysicalDiskInfo> diskInfo;
            Task<HardwareMonitor>? monitorTask = null;
            try
            {
                await WriteProgress(writer,
                    loading: "cpu",
                    done: Array.Empty<string>(),
                    pending: new[] { "ram" }.Concat(HardwareMonitor.StagedCategories).ToArray());

                // Computer.Open cannot be cancelled mid-flight; WaitAsync lets
                // a session end abandon it (AbandonMonitorInit observes and
                // disposes the orphan; worst case process exit cleans up).
                monitorTask = Task.Run(() => new HardwareMonitor(staged: true), CancellationToken.None);
                var diskTask = Task.Run(DiskInfoReader.Read, CancellationToken.None);
                await Task.WhenAll(monitorTask, diskTask).WaitAsync(sessionCt);

                monitorRef = new MonitorRef(monitorTask.Result);
                monitorTask = null; // ownership moved: the finally disposes via monitorRef
                diskInfo = diskTask.Result;

                // The ONE refresh before the first snapshot line (SND-3): the
                // PawnIO probe and the first snapshot share this hardware
                // list.
                firstHardware = monitorRef.Monitor.Refresh();
                _health = RunProbe(firstHardware);
                _log.LogInformation("PawnIO probe: {Pawnio} (degraded={Degraded})",
                    _health.Pawnio, _health.Degraded);
            }
            catch (OperationCanceledException)
            {
                _log.LogInformation("Session ended during sensor init");
                AbandonMonitorInit(monitorTask);
                return;
            }
            catch (IOException)
            {
                _log.LogInformation("Client disconnected during sensor init");
                AbandonMonitorInit(monitorTask);
                return;
            }
            catch (Exception ex)
            {
                // SND-1: a throw during init must surface as a degraded health
                // snapshot + non-zero exit, never a silent disappearance.
                _log.LogError(ex, "sensor init failed");
                FatalInit = true;
                AbandonMonitorInit(monitorTask);
                await TryWriteFatalHealth(pipe, pawnio: "denied",
                    notes: $"sensor init failed: {ex.Message}");
                return;
            }

            await ServeOne(writer, monitorRef, firstHardware, diskInfo, sessionCt);
            _log.LogInformation("Client disconnected, stopping service");
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            // Last-resort safety net (SND-1): a worker crash must leave a log
            // trail and, while a client is still attached, a degraded health
            // snapshot.
            _log.LogError(ex, "sensor worker crashed");
            FatalInit = true;
            if (pipe is { IsConnected: true })
                await TryWriteFatalHealth(pipe, pawnio: "denied",
                    notes: $"sensor worker crashed: {ex.Message}");
        }
        finally
        {
            // Covers session ends that never pass through RequestSessionEnd
            // (write IOException in the serve loop, accept timeout): teardown
            // below plus host stop must finish within the grace period.
            ArmTeardownWatchdog();

            // Defensive disposal: any throw in finally would mask the original
            // exception (or, if there is none, crash the host on a clean
            // shutdown). LHM's Computer.Close and a broken-pipe flush are both
            // known throwers.
            try { monitorRef?.Monitor.Dispose(); }
            catch (Exception ex) { _log.LogWarning(ex, "HardwareMonitor.Dispose threw"); }

            if (writer is not null)
            {
                try { writer.Dispose(); }
                catch (IOException) { /* peer gone */ }
            }
            if (pipe is not null)
            {
                try { await pipe.DisposeAsync(); }
                catch (IOException) { /* peer gone */ }
                catch (Exception ex) { _log.LogWarning(ex, "pipe.DisposeAsync threw"); }
            }
            // Pipe disposal unblocks the control reader's ReadLine; awaiting
            // the task observes any fault so nothing surfaces as an
            // unobserved exception after the session.
            if (controlTask is not null)
            {
                try { await controlTask.WaitAsync(TimeSpan.FromSeconds(2)); }
                catch (Exception ex) { _log.LogDebug(ex, "control reader did not end cleanly"); }
            }
            clientCts?.Dispose();
            // BackgroundService.ExecuteAsync completing normally does NOT stop
            // the host in .NET 8 — the process would idle forever with no
            // hosted work. Explicitly request shutdown so the service exits
            // as soon as the worker returns (client disconnect, shutdown
            // message, or 30 s startup timeout).
            _lifetime.StopApplication();
        }
    }

    /// <summary>
    /// Health-only snapshot — exists because a silent exit on init failure
    /// leaves the dashboard with no signal beyond a pipe disconnect, and "the
    /// service started and then disappeared" is one of the hardest crash
    /// classes to triage. Best-effort: a client that has already detached, a
    /// JSON serializer fault or a kernel write error must not become a second
    /// crash on top of the original one, so the catch swallows — but it does
    /// emit one warning so the Event Log explains why no banner reached the
    /// user.
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
        StreamWriter writer,
        MonitorRef monitorRef,
        IReadOnlyList<IHardware> firstHardware,
        IReadOnlyDictionary<int, PhysicalDiskInfo> diskInfo,
        CancellationToken sessionCt)
    {
        int tickCounter = 0;
        var currentDiskInfo = diskInfo;
        var linkSpeeds = LinkSpeeds();
        // The init refresh already produced this list; refreshing again here
        // would double the cost of the first snapshot.
        IReadOnlyList<IHardware>? carried = firstHardware;
        // No topology baseline until every category is enabled: hardware
        // groups arriving through staged init must not read as hot-plug
        // churn (SND-7).
        TopologyDebounce? topology = monitorRef.Monitor.PendingCategories.Count == 0
            ? new TopologyDebounce(TopologySignature.IdentifierSet(monitorRef.Monitor.List()))
            : null;

        while (!sessionCt.IsCancellationRequested)
        {
            try
            {
                IReadOnlyList<IHardware> hardware = carried ?? monitorRef.Monitor.Refresh();
                carried = null;
                // Refresh is synchronous and can run for seconds during staged
                // init; re-check before building/sending another snapshot.
                if (sessionCt.IsCancellationRequested) return;

                if (++tickCounter >= 5)
                {
                    tickCounter = 0;
                    // SND-12: adapter enumeration is cheap but not free;
                    // every 5th tick tracks link changes closely enough.
                    linkSpeeds = LinkSpeeds();
                    if (topology is not null)
                    {
                        var newIds = TopologySignature.IdentifierSet(hardware);
                        if (topology.ShouldRebuild(newIds))
                        {
                            var added = string.Join(",", newIds.Except(topology.Baseline));
                            var removed = string.Join(",", topology.Baseline.Except(newIds));
                            _log.LogInformation(
                                "Topology change confirmed (added=[{Added}] removed=[{Removed}]); recreating monitor",
                                added, removed);

                            // Construct first so a failure keeps the old
                            // monitor; the debounce re-arms and retries on a
                            // later check.
                            HardwareMonitor? newMonitor = null;
                            try
                            {
                                newMonitor = new HardwareMonitor();
                            }
                            catch (Exception ctorEx) when (ctorEx is not OperationCanceledException)
                            {
                                _log.LogWarning(ctorEx,
                                    "topology-change monitor reconstruction failed; keeping previous monitor");
                            }

                            if (newMonitor is not null)
                            {
                                var oldMonitor = monitorRef.Monitor;
                                monitorRef.Monitor = newMonitor;
                                oldMonitor.Dispose();
                                currentDiskInfo = DiskInfoReader.Read();
                                hardware = monitorRef.Monitor.Refresh();
                                topology.Rebaseline(TopologySignature.IdentifierSet(hardware));
                            }
                        }
                    }
                }

                Snapshot snap = SnapshotBuilder.Build(
                    hardware, PagefileReader.Read(), linkSpeeds, currentDiskInfo)
                    with
                { Health = _health };
                string json = JsonSerializer.Serialize(snap, SensordJsonContext.Default.Snapshot);
                await writer.WriteLineAsync(json);
                // Don't start a multi-second category enable for a client
                // that has already asked to shut down.
                if (sessionCt.IsCancellationRequested) return;

                if (monitorRef.Monitor.PendingCategories.Count > 0)
                {
                    // Enabling must happen on this thread: Computer.Hardware
                    // traversal is not safe against concurrent group mutation.
                    if (monitorRef.Monitor.EnableNextCategory(out string enabled, out Exception? enableError))
                    {
                        if (enableError is null)
                            _log.LogInformation("Sensor category {Category} enabled", enabled);
                        else
                            _log.LogWarning(enableError,
                                "Sensor category {Category} failed to enable; skipped", enabled);
                        await WriteProgress(writer, monitorRef.Monitor);
                    }
                    if (monitorRef.Monitor.PendingCategories.Count == 0)
                    {
                        topology = new TopologyDebounce(
                            TopologySignature.IdentifierSet(monitorRef.Monitor.List()));
                        // Staged init churns through WMI/COM/SPD enumeration
                        // garbage; one aggressive collect here hands that
                        // memory back to the OS instead of holding tens of MB
                        // for the rest of the session. Runs once, between
                        // snapshots, so the pause is invisible to the client.
                        GC.Collect(GC.MaxGeneration, GCCollectionMode.Aggressive,
                            blocking: true, compacting: true);
                    }
                    // No inter-snapshot delay while categories are pending:
                    // the full sensor set should land as fast as the
                    // hardware allows.
                    continue;
                }

                await Task.Delay(_intervalMs, sessionCt);
            }
            catch (IOException) { return; }
            catch (OperationCanceledException)
            {
                // Task.Delay observed the session cancellation; shutdown requested.
                return;
            }
            catch (Exception ex)
            {
                _log.LogError(ex, "snapshot emit error");
                try { await Task.Delay(1000, sessionCt); }
                catch (OperationCanceledException) { return; }
            }
        }
    }

    private void ReadControlLoop(StreamReader reader, CancellationToken token, Action onSessionEnd)
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
                    onSessionEnd();
                    return;
                }
                if (msg.IntervalMs is int ms && ms is >= 250 and <= 60_000)
                {
                    Interlocked.Exchange(ref _intervalMs, ms);
                    _log.LogInformation("Interval changed to {Ms} ms", ms);
                }
            }
            // ReadLine() == null means client EOF: end the session here
            // instead of letting the serve loop discover the broken pipe one
            // write later (SND-6).
            onSessionEnd();
        }
        catch (IOException)
        {
            // Client dropped mid-read — same as EOF.
            onSessionEnd();
        }
        catch (ObjectDisposedException)
        {
            // Session teardown disposed the pipe under the blocking ReadLine.
        }
    }

    /// <summary>
    /// The service contract is "nothing keeps running once the client is
    /// gone". Teardown crosses several calls that cannot observe cancellation
    /// (synchronous LHM refresh/enable, Computer.Close, host stop), so once
    /// the session ends a hard deadline guarantees process exit even if one
    /// of them stalls. Normal teardown finishes well inside the grace period
    /// and the armed task simply dies with the process.
    /// </summary>
    private void ArmTeardownWatchdog()
    {
        if (Interlocked.Exchange(ref _teardownWatchdogArmed, 1) == 1) return;
        _ = Task.Run(async () =>
        {
            await Task.Delay(TeardownGrace);
            _log.LogWarning(
                "Teardown still running {Seconds:0.0} s after session end; forcing process exit",
                TeardownGrace.TotalSeconds);
            Environment.Exit(FatalInit ? 1 : 0);
        });
    }

    /// <summary>
    /// A staged Computer.Open cannot be cancelled; when the session ends
    /// first, the construction task is abandoned. The continuation disposes a
    /// late success and observes a late fault so neither leaks past the
    /// session (the host may already be exiting — best effort by design).
    /// </summary>
    private static void AbandonMonitorInit(Task<HardwareMonitor>? monitorTask)
    {
        monitorTask?.ContinueWith(
            static t =>
            {
                if (t.IsCompletedSuccessfully)
                {
                    try { t.Result.Dispose(); }
                    catch (Exception) { /* process is exiting; best effort */ }
                }
                else
                {
                    _ = t.Exception;
                }
            },
            CancellationToken.None,
            TaskContinuationOptions.ExecuteSynchronously,
            TaskScheduler.Default);
    }

    /// <summary>
    /// <c>loading</c> is the category whose enable runs next — the client
    /// renders it as in-flight while the serve loop works on it; <c>pending</c>
    /// excludes it. The last enable therefore emits the protocol's final line
    /// (loading null, pending empty) with no separate emission.
    /// </summary>
    private static async Task WriteProgress(StreamWriter writer, HardwareMonitor monitor)
    {
        var pending = monitor.PendingCategories;
        string? loading = pending.Count > 0 ? pending[0] : null;
        IReadOnlyList<string> tail = pending.Count > 1
            ? pending.Skip(1).ToArray()
            : Array.Empty<string>();
        await WriteProgress(writer, loading, monitor.DoneCategories, tail);
    }

    private static async Task WriteProgress(
        StreamWriter writer, string? loading, IReadOnlyList<string> done, IReadOnlyList<string> pending)
    {
        var msg = new ProgressMessage(new ProgressInfo(loading, done, pending));
        string json = JsonSerializer.Serialize(msg, SensordJsonContext.Default.ProgressMessage);
        await writer.WriteLineAsync(json);
    }

    private static Dictionary<string, long> LinkSpeeds()
    {
        var map = new Dictionary<string, long>();
        foreach (var ni in NetworkInterface.GetAllNetworkInterfaces())
            if (ni.OperationalStatus == OperationalStatus.Up)
                map[ni.Name] = ni.Speed;
        return map;
    }

    /// <summary>
    /// CPU temperatures specifically (hardware type Cpu): they are the only
    /// reading that proves the PawnIO/MSR path works — board or drive
    /// thermals come from other chips and would mask a dead driver (SND-5).
    /// </summary>
    internal static HealthInfo RunProbe(IReadOnlyList<IHardware> hardware)
    {
        try
        {
            var cpuTemps = new List<double?>();
            foreach (var h in hardware)
            {
                if (h.HardwareType != HardwareType.Cpu) continue;
                foreach (var s in h.Sensors)
                    if (s.SensorType == SensorType.Temperature)
                        cpuTemps.Add(s.Value.HasValue ? (double?)s.Value.Value : null);
            }
            return HealthProbe.Classify(cpuTemps, exception: null);
        }
        catch (Exception ex)
        {
            return HealthProbe.Classify(cpuTemps: null, exception: ex);
        }
    }
}
