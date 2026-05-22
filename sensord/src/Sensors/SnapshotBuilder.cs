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
                PowerW: hw.Val(SensorType.Power, "GPU"),
                MemoryLoad: hw.Val(SensorType.Load, "GPU Memory Controller")));
        }
        return PreferDiscreteGpus(gpus);
    }

    /// <summary>
    /// Drops integrated GPUs when a discrete GPU is present — beside a real GPU
    /// the integrated one is redundant clutter (no temperature, no clock, only
    /// shared memory). Integrated GPUs are kept only as a fallback for machines
    /// that have no discrete GPU at all.
    /// </summary>
    internal static List<GpuInfo> PreferDiscreteGpus(List<GpuInfo> gpus)
    {
        if (gpus.Any(g => g.Kind == "discrete"))
            gpus.RemoveAll(g => g.Kind == "integrated");
        return gpus;
    }

    public static RamInfo BuildRam(IHardware? memory, PagefileInfo pf)
    {
        double? usedGb = memory?.Val(SensorType.Data, "Memory Used");
        double? availGb = memory?.Val(SensorType.Data, "Memory Available");
        return new RamInfo(
            UsedMb: usedGb is double u ? u * 1024 : null,
            TotalMb: usedGb is double ut && availGb is double a ? (ut + a) * 1024 : null,
            AvailableMb: availGb is double av ? av * 1024 : null,
            Load: memory?.Val(SensorType.Load, "Memory"),
            CachedMb: pf.CachedMb,
            PagefileUsedMb: pf.UsedMb,
            PagefileTotalMb: pf.TotalMb);
    }

    public static List<StorageInfo> BuildStorage(IEnumerable<IHardware> hardware,
                                                  IReadOnlyDictionary<int, PhysicalDiskInfo> diskInfo)
    {
        var list = new List<StorageInfo>();
        foreach (var hw in hardware.Where(h => h.HardwareType == HardwareType.Storage))
        {
            double? usedPct = hw.Val(SensorType.Load, "Used Space");

            // LHM identifier looks like "/nvme/0" or "/hdd/2"; the trailing integer is
            // the Windows physical-disk number that matches MSFT_PhysicalDisk.DeviceId.
            string idStr = hw.Identifier.ToString() ?? string.Empty;
            int lastSlash = idStr.LastIndexOf('/');
            int diskIndex = -1;
            bool hasDiskIndex = lastSlash >= 0
                && int.TryParse(idStr.AsSpan(lastSlash + 1), out diskIndex);

            string kind;
            double? totalGb;
            double? usedGb;

            if (hasDiskIndex && diskInfo.TryGetValue(diskIndex, out PhysicalDiskInfo info))
            {
                kind    = info.Kind;
                totalGb = info.TotalGb;
                usedGb  = (info.TotalGb.HasValue && usedPct.HasValue) ? info.TotalGb.Value * usedPct.Value / 100.0 : null;
            }
            else
            {
                // WMI data unavailable — fall back to the identifier prefix for kind,
                // leave capacity null.
                kind    = ClassifyDiskByIdentifier(idStr);
                totalGb = null;
                usedGb  = null;
            }

            list.Add(new StorageInfo(
                Name: hw.Name,
                Kind: kind,
                Temp: hw.FirstVal(SensorType.Temperature),
                Activity: hw.Val(SensorType.Load, "Total Activity"),
                UsedGb: usedGb,
                TotalGb: totalGb));
        }
        return list;
    }

    /// <summary>
    /// Fallback kind classification using the LHM identifier prefix
    /// (<c>/nvme/…</c>, <c>/ssd/…</c>, <c>/hdd/…</c>) when WMI data is unavailable.
    /// </summary>
    internal static string ClassifyDiskByIdentifier(string identifier)
    {
        if (identifier.Contains("/nvme/", StringComparison.OrdinalIgnoreCase)) return "nvme";
        if (identifier.Contains("/ssd/",  StringComparison.OrdinalIgnoreCase)) return "ssd";
        return "hdd";
    }

    public static BoardInfo? BuildBoard(IHardware? motherboard)
    {
        var io = motherboard?.SubHardware.FirstOrDefault();
        if (io is null) return null;
        return new BoardInfo(
            Temp: io.Val(SensorType.Temperature, "System")
               ?? io.Val(SensorType.Temperature, "Motherboard")
               ?? io.FirstVal(SensorType.Temperature),
            VrmTemp: io.Val(SensorType.Temperature, "VRM"));
    }

    public static (List<FanInfo> fans, List<VoltageInfo> voltages) BuildBoardSensors(IHardware? motherboard)
    {
        var fans = new List<FanInfo>();
        var voltages = new List<VoltageInfo>();
        var io = motherboard?.SubHardware.FirstOrDefault();
        if (io is not null)
        {
            foreach (var s in io.Sensors)
            {
                if (s.SensorType == SensorType.Fan && s.Value is float rpm)
                    fans.Add(new FanInfo(s.Name, rpm));
                else if (s.SensorType == SensorType.Voltage && s.Value is float v)
                    voltages.Add(new VoltageInfo(s.Name, v));
            }
        }
        return (fans, voltages);
    }

    public static NetInfo? BuildNet(IEnumerable<IHardware> hardware, IReadOnlyDictionary<string, long> linkSpeeds)
    {
        IHardware? best = null;
        double bestThroughput = -1;
        foreach (var hw in hardware.Where(h => h.HardwareType == HardwareType.Network))
        {
            double down = hw.Val(SensorType.Throughput, "Download") ?? 0;
            double up = hw.Val(SensorType.Throughput, "Upload") ?? 0;
            if (down + up > bestThroughput) { bestThroughput = down + up; best = hw; }
        }
        if (best is null) return null;

        double downBps = best.Val(SensorType.Throughput, "Download") ?? 0;
        double upBps = best.Val(SensorType.Throughput, "Upload") ?? 0;
        long linkBps = linkSpeeds.TryGetValue(best.Name, out long ls) ? ls : 0;
        return new NetInfo(
            Adapter: best.Name,
            DownBps: downBps,
            UpBps: upBps,
            LinkBps: linkBps > 0 ? linkBps : null,
            DownPct: linkBps > 0 ? NetUtil.Utilisation(downBps, linkBps) : null,
            UpPct:   linkBps > 0 ? NetUtil.Utilisation(upBps, linkBps)   : null);
    }

    public static Snapshot Build(IReadOnlyList<IHardware> hardware, PagefileInfo pf,
                                 IReadOnlyDictionary<string, long> linkSpeeds,
                                 IReadOnlyDictionary<int, PhysicalDiskInfo> diskInfo)
    {
        var cpuHw = hardware.FirstOrDefault(h => h.HardwareType == HardwareType.Cpu);
        var memHw = hardware.FirstOrDefault(h => h.HardwareType == HardwareType.Memory);
        var boardHw = hardware.FirstOrDefault(h => h.HardwareType == HardwareType.Motherboard);
        var (fans, voltages) = BuildBoardSensors(boardHw);
        var gpus = BuildGpus(hardware);
        var storage = BuildStorage(hardware, diskInfo);

        return new Snapshot(
            Version: 1,
            Timestamp: DateTimeOffset.UtcNow.ToUnixTimeSeconds(),
            Cpu: cpuHw is null ? null : BuildCpu(cpuHw),
            Gpu: gpus.Count > 0 ? gpus : null,
            Ram: BuildRam(memHw, pf),
            Storage: storage.Count > 0 ? storage : null,
            Board: BuildBoard(boardHw),
            Fans: fans.Count > 0 ? fans : null,
            Voltages: voltages.Count > 0 ? voltages : null,
            Net: BuildNet(hardware, linkSpeeds));
    }
}
