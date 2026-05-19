using System.Net.NetworkInformation;
using System.Text.Json;
using Sensord.Control;
using Sensord.Model;
using Sensord.Sensors;

namespace Sensord;

internal static class Program
{
    private static volatile int _intervalMs = 1000;
    private static volatile bool _running = true;

    private static void Main()
    {
        var stdout = Console.OpenStandardOutput();
        using var writer = new StreamWriter(stdout) { AutoFlush = true };

        var stdinThread = new Thread(ReadControl) { IsBackground = true };
        stdinThread.Start();

        using var monitor = new HardwareMonitor();
        while (_running)
        {
            try
            {
                var hardware = monitor.Refresh();
                Snapshot snap = SnapshotBuilder.Build(hardware, PagefileReader.Read(), LinkSpeeds());
                writer.WriteLine(JsonSerializer.Serialize(snap, SensordJsonContext.Default.Snapshot));
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"sensord: poll error: {ex.Message}");
            }
            Thread.Sleep(_intervalMs);
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
    }

    private static Dictionary<string, long> LinkSpeeds()
    {
        var map = new Dictionary<string, long>();
        foreach (var ni in NetworkInterface.GetAllNetworkInterfaces())
            if (ni.OperationalStatus == OperationalStatus.Up)
                map[ni.Name] = ni.Speed;
        return map;
    }
}
