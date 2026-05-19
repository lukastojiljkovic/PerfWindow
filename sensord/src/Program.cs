using System.Text.Json;
using Sensord.Model;
using Sensord.Sensors;

namespace Sensord;

internal static class Program
{
    private static void Main()
    {
        using var monitor = new HardwareMonitor();
        var hardware = monitor.Refresh();
        Snapshot snap = SnapshotBuilder.Build(hardware, PagefileReader.Read(),
            new Dictionary<string, long>());
        Console.Error.WriteLine(JsonSerializer.Serialize(snap, SensordJsonContext.Default.Snapshot));
    }
}
