using Sensord.Sensors;

namespace Sensord;

internal static class Program
{
    private static void Main()
    {
        using var monitor = new HardwareMonitor();
        foreach (var hw in monitor.Refresh())
        {
            Console.Error.WriteLine($"{hw.HardwareType}: {hw.Name} ({hw.Sensors.Length} sensors)");
            foreach (var sub in hw.SubHardware)
                Console.Error.WriteLine($"  sub {sub.HardwareType}: {sub.Name} ({sub.Sensors.Length} sensors)");
        }
    }
}
