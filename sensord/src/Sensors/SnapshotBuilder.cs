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

        var coreTemps = cpu.Sensors
            .Where(s => s.SensorType == SensorType.Temperature && s.Name.StartsWith("CPU Core #"))
            .OrderBy(s => s.Index)
            .Select(s => (double?)s.Value)
            .ToList();

        double? temp = cpu.Val(SensorType.Temperature, "Package")
                    ?? cpu.Val(SensorType.Temperature, "Core (Tctl")
                    ?? cpu.FirstVal(SensorType.Temperature);

        double? clock = cpu.Sensors
            .Where(s => s.SensorType == SensorType.Clock && s.Name.StartsWith("CPU Core"))
            .Select(s => (double?)s.Value)
            .DefaultIfEmpty(null)
            .Max();

        // Vcore / VID is exposed under several vendor-specific names. Try the
        // common ones in order; first hit wins.
        double? voltage = cpu.Val(SensorType.Voltage, "Vcore")
                       ?? cpu.Val(SensorType.Voltage, "CPU VID")
                       ?? cpu.Val(SensorType.Voltage, "CPU Core");

        return new CpuInfo(
            Name: cpu.Name,
            Load: cpu.Val(SensorType.Load, "CPU Total"),
            Cores: cores.Count > 0 ? cores : null,
            Temp: temp,
            ClockMhz: clock,
            PowerW: cpu.Val(SensorType.Power, "Package"),
            CoreTemps: coreTemps.Count > 0 ? coreTemps : null,
            VoltageV: voltage);
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

            // Vendor drivers name the GPU die hot-spot reading differently.
            // NVIDIA: "GPU Hot Spot"; AMD: "GPU Hot Spot" or "GPU Junction".
            double? hotSpot = hw.Val(SensorType.Temperature, "GPU Hot Spot")
                           ?? hw.Val(SensorType.Temperature, "GPU Junction");

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
                MemoryLoad: hw.Val(SensorType.Load, "GPU Memory Controller"),
                HotSpotTempC: hotSpot));
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
                kind = info.Kind;
                totalGb = info.TotalGb;
                usedGb = (info.TotalGb.HasValue && usedPct.HasValue) ? info.TotalGb.Value * usedPct.Value / 100.0 : null;
            }
            else
            {
                // WMI data unavailable — fall back to the identifier prefix for kind,
                // leave capacity null.
                kind = ClassifyDiskByIdentifier(idStr);
                totalGb = null;
                usedGb = null;
            }

            // LHM exposes drive wear as a `Level` sensor. "Remaining Life" is
            // the common name across SATA SSD/HDD and most NVMe drivers; some
            // NVMe-only firmwares emit only the NVMe-spec "Available Spare",
            // which is semantically identical (0–100 %, 100 = new). We
            // intentionally skip "Percentage Used" (inverse semantics) to keep
            // the JSON contract single-meaning.
            double? health = hw.Val(SensorType.Level, "Remaining Life")
                          ?? hw.Val(SensorType.Level, "Available Spare");

            // Per-drive throughput, bytes/second.
            double? readBps = hw.Val(SensorType.Throughput, "Read Rate");
            double? writeBps = hw.Val(SensorType.Throughput, "Write Rate");

            list.Add(new StorageInfo(
                Name: hw.Name,
                Kind: kind,
                Temp: hw.FirstVal(SensorType.Temperature),
                Activity: hw.Val(SensorType.Load, "Total Activity"),
                UsedGb: usedGb,
                TotalGb: totalGb,
                Health: health,
                ReadBps: readBps,
                WriteBps: writeBps));
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
        if (identifier.Contains("/ssd/", StringComparison.OrdinalIgnoreCase)) return "ssd";
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

    /// <summary>
    /// Picks the first <c>HardwareType.Battery</c> from the LHM inventory and
    /// flattens its sensors into a <see cref="BatteryInfo"/>. Returns
    /// <c>null</c> on desktops (no battery hardware present).
    /// </summary>
    /// <remarks>
    /// LHM exposes charging and discharging as two separate <c>Power</c>
    /// sensors. We collapse them into a single signed <c>rate_w</c>:
    /// positive while charging, negative while discharging. Capacities are
    /// reported in mWh under <c>SensorType.Energy</c>.
    /// </remarks>
    public static BatteryInfo? BuildBattery(IEnumerable<IHardware> hardware)
    {
        var batt = hardware.FirstOrDefault(h => h.HardwareType == HardwareType.Battery);
        if (batt is null) return null;

        // LHM exposes one combined "Charge/Discharge Rate" power sensor that is
        // positive while charging and negative while discharging. Older driver
        // builds report two separate "Charge Rate" / "Discharge Rate" sensors;
        // try the combined form first, then collapse the split form into the
        // same signed convention.
        double? combinedRate = batt.Val(SensorType.Power, "Charge/Discharge Rate");
        double? rateW = combinedRate;
        if (rateW is null)
        {
            double? dischargeRate = FindPowerExact(batt, "Discharge Rate");
            double? chargeRate = FindPowerExact(batt, "Charge Rate");
            rateW = dischargeRate.HasValue ? -dischargeRate.Value
                  : chargeRate.HasValue ? chargeRate.Value
                  : (double?)null;
        }

        double? remainingSecRaw = batt.Val(SensorType.TimeSpan, "Remaining Time (Estimated)");
        long? remainingSec = remainingSecRaw.HasValue ? (long)remainingSecRaw.Value : (long?)null;

        // LHM names "Fully-Charged Capacity" (with the hyphen); the older
        // "Full Charged Capacity" form is kept as a fallback for safety.
        double? fullCap = batt.Val(SensorType.Energy, "Fully-Charged Capacity")
                       ?? batt.Val(SensorType.Energy, "Full Charged Capacity");

        return new BatteryInfo(
            ChargePct: batt.Val(SensorType.Level, "Charge Level"),
            RateW: rateW,
            VoltageV: batt.Val(SensorType.Voltage, "Voltage"),
            DesignCapacityMwh: batt.Val(SensorType.Energy, "Designed Capacity"),
            FullCapacityMwh: fullCap,
            TimeRemainingSec: remainingSec);
    }

    /// <summary>
    /// Exact-name <c>SensorType.Power</c> lookup. Used in <see cref="BuildBattery"/>
    /// because the substring-matching default would mis-attribute a probe for
    /// "Charge Rate" to a "Discharge Rate" sensor.
    /// </summary>
    private static double? FindPowerExact(IHardware hw, string exactName)
    {
        foreach (var s in hw.Sensors)
        {
            if (s.SensorType == SensorType.Power
                && s.Name.Equals(exactName, StringComparison.OrdinalIgnoreCase)
                && s.Value is float v)
            {
                return v;
            }
        }
        return null;
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
            UpPct: linkBps > 0 ? NetUtil.Utilisation(upBps, linkBps) : null);
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
            Net: BuildNet(hardware, linkSpeeds),
            Battery: BuildBattery(hardware),
            UptimeSec: Environment.TickCount64 / 1000);
    }
}
