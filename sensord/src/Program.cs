using LibreHardwareMonitor.Hardware;
using Sensord.Sensors;

namespace Sensord;

internal static class Program
{
    private static void Main()
    {
        using var monitor = new HardwareMonitor();
        var hardware = monitor.Refresh();

        var cpu = hardware.FirstOrDefault(h => h.HardwareType == HardwareType.Cpu);
        if (cpu is not null)
            Console.Error.WriteLine($"CPU: {System.Text.Json.JsonSerializer.Serialize(SnapshotBuilder.BuildCpu(cpu))}");

        foreach (var gpu in SnapshotBuilder.BuildGpus(hardware))
            Console.Error.WriteLine($"GPU: {System.Text.Json.JsonSerializer.Serialize(gpu)}");
    }
}
