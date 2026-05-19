using LibreHardwareMonitor.Hardware;
using Sensord.Model;

namespace Sensord.Sensors;

public static class SnapshotBuilder
{
    public static CpuInfo BuildCpu(IHardware cpu)
    {
        var cores = cpu.Sensors
            .Where(s => s.SensorType == SensorType.Load && s.Name.StartsWith("CPU Core #"))
            .OrderBy(s => s.Index)
            .Select(s => (double)(s.Value ?? 0f))
            .ToList();

        double? temp = cpu.Val(SensorType.Temperature, "Package")
                    ?? cpu.Val(SensorType.Temperature, "Core (Tctl")
                    ?? cpu.FirstVal(SensorType.Temperature);

        double? clock = cpu.Sensors
            .Where(s => s.SensorType == SensorType.Clock && s.Name.StartsWith("CPU Core"))
            .Select(s => (double?)s.Value)
            .DefaultIfEmpty(null)
            .Max();

        return new CpuInfo(
            Name: cpu.Name,
            Load: cpu.Val(SensorType.Load, "CPU Total"),
            Cores: cores.Count > 0 ? cores : null,
            Temp: temp,
            ClockMhz: clock,
            PowerW: cpu.Val(SensorType.Power, "Package"));
    }

    public static List<GpuInfo> BuildGpus(IEnumerable<IHardware> hardware)
    {
        var gpus = new List<GpuInfo>();
        foreach (var hw in hardware)
        {
            if (hw.HardwareType is not (HardwareType.GpuNvidia or HardwareType.GpuAmd or HardwareType.GpuIntel))
                continue;

            bool integrated = hw.HardwareType == HardwareType.GpuIntel
                || hw.Name.Contains("Radeon Graphics", StringComparison.OrdinalIgnoreCase)
                || hw.Name.Contains("Vega", StringComparison.OrdinalIgnoreCase);

            gpus.Add(new GpuInfo(
                Name: hw.Name,
                Kind: integrated ? "integrated" : "discrete",
                Load: hw.Val(SensorType.Load, "GPU Core"),
                Temp: hw.Val(SensorType.Temperature, "GPU Core"),
                VramUsedMb: hw.Val(SensorType.SmallData, "GPU Memory Used"),
                VramTotalMb: hw.Val(SensorType.SmallData, "GPU Memory Total"),
                ClockMhz: hw.Val(SensorType.Clock, "GPU Core"),
                FanRpm: hw.Val(SensorType.Fan, "GPU"),
                PowerW: hw.Val(SensorType.Power, "GPU")));
        }
        return gpus;
    }
}
