using System.Net.NetworkInformation;
using System.Text.Json;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Hosting.WindowsServices;
using Microsoft.Extensions.Logging;
using Sensord.Control;
using Sensord.Model;
using Sensord.Sensors;
using Sensord.Service;

namespace Sensord;

internal static class Program
{
    private static volatile int _intervalMs = 1000;
    private static volatile bool _running = true;
    private static readonly ManualResetEventSlim _shutdown = new(false);

    private static int Main(string[] args)
    {
        RedirectDriverTempDir();

        if (args.Contains("--probe"))
        {
            HardwareProbe.Run(Console.Out);
            return 0;
        }

        if (args.Contains("--service"))
        {
            string pipeName = ExtractPipeName(args) ?? PipeServer.DefaultPipeName;
            return RunService(pipeName);
        }

        RunConsoleChild();
        return 0;
    }

    private static int RunService(string pipeName)
    {
        var builder = Host.CreateApplicationBuilder();
        builder.Services.AddWindowsService(options =>
        {
            options.ServiceName = "PerfWindowSensor";
        });
        builder.Logging.AddEventLog(settings =>
        {
            settings.SourceName = "PerfWindowSensor";
        });
        builder.Services.AddSingleton<SensorPipeWorker>(sp =>
            new SensorPipeWorker(sp.GetRequiredService<ILogger<SensorPipeWorker>>(), pipeName));
        builder.Services.AddHostedService(sp => sp.GetRequiredService<SensorPipeWorker>());
        builder.Build().Run();
        return 0;
    }

    /// <summary>Extract the value of <c>--pipe-name X</c> from args, or <c>null</c>.</summary>
    private static string? ExtractPipeName(string[] args)
    {
        for (int i = 0; i < args.Length - 1; i++)
            if (args[i] == "--pipe-name")
                return args[i + 1];
        return null;
    }

    private static void RunConsoleChild()
    {
        var stdout = Console.OpenStandardOutput();
        using var writer = new StreamWriter(stdout) { AutoFlush = true };

        var stdinThread = new Thread(ReadControl) { IsBackground = true };
        stdinThread.Start();

        var diskInfo = DiskInfoReader.Read();
        using var monitor = new HardwareMonitor();
        do
        {
            try
            {
                var hardware = monitor.Refresh();
                Snapshot snap = SnapshotBuilder.Build(hardware, PagefileReader.Read(), LinkSpeeds(), diskInfo);
                writer.WriteLine(JsonSerializer.Serialize(snap, SensordJsonContext.Default.Snapshot));
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"sensord: poll error: {ex.Message}");
            }
            _shutdown.Wait(_intervalMs);
        } while (_running);
    }

    /// <summary>
    /// Points this process's TMP/TEMP at a PerfWindow-owned directory.
    /// LibreHardwareMonitor extracts its WinRing0 kernel driver next to the
    /// executable; only if that fails does it fall back to a temp-directory
    /// file. WinRing0 is on Microsoft's vulnerable-driver list, so in the
    /// normal user temp folder Windows Defender quarantines it on sight. This
    /// redirects that fallback into <c>%LOCALAPPDATA%\PerfWindow\driver</c> — a
    /// folder the setup step excludes — so neither the primary location (next
    /// to the executable) nor the fallback is ever scanned. Process-scoped —
    /// the machine's temp configuration is untouched.
    /// </summary>
    private static void RedirectDriverTempDir()
    {
        try
        {
            string localAppData =
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            if (string.IsNullOrEmpty(localAppData))
                return;

            string dir = Path.Combine(localAppData, "PerfWindow", "driver");
            Directory.CreateDirectory(dir);
            Environment.SetEnvironmentVariable("TMP", dir);
            Environment.SetEnvironmentVariable("TEMP", dir);
        }
        catch (Exception ex)
        {
            // A failure here only means the driver falls back to the user temp
            // directory (the prior behaviour) — it must never stop sensord.
            Console.Error.WriteLine($"sensord: temp-dir redirect failed: {ex.Message}");
        }
    }

    /// <summary>Reads control lines from stdin; an EOF means the dashboard has exited.</summary>
    private static void ReadControl()
    {
        string? line;
        while ((line = Console.In.ReadLine()) is not null)
        {
            ControlMessage? msg = ControlReader.Parse(line);
            if (msg?.IntervalMs is int ms && ms is >= 250 and <= 60_000)
                _intervalMs = ms;
        }
        _running = false;   // stdin closed -> stop the poll loop
        _shutdown.Set();    // interrupt the interval wait for a prompt exit
    }

    private static Dictionary<string, long> LinkSpeeds()
    {
        // Keys on NetworkInterface.Name, which BuildNet looks up by IHardware.Name.
        // The two names match on this dev machine; an unmatched adapter degrades
        // gracefully to link_bps: null and null down_pct/up_pct.
        var map = new Dictionary<string, long>();
        foreach (var ni in NetworkInterface.GetAllNetworkInterfaces())
            if (ni.OperationalStatus == OperationalStatus.Up)
                map[ni.Name] = ni.Speed;
        return map;
    }
}
