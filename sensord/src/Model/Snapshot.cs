using System.Text.Json.Serialization;

namespace Sensord.Model;

/// <summary>
/// Top-level NDJSON snapshot emitted once per poll interval on stdout. The
/// schema is additive: dashboard builds tolerate older sensord drops via
/// <c>#[serde(default)]</c>; sensord drops never remove a field once shipped.
/// </summary>
public record Snapshot(
    [property: JsonPropertyName("v")] int Version,
    [property: JsonPropertyName("ts")] long Timestamp,
    [property: JsonPropertyName("cpu")] CpuInfo? Cpu,
    [property: JsonPropertyName("gpu")] IReadOnlyList<GpuInfo>? Gpu,
    [property: JsonPropertyName("igpu")] GpuInfo? Igpu,
    [property: JsonPropertyName("ram")] RamInfo? Ram,
    [property: JsonPropertyName("storage")] IReadOnlyList<StorageInfo>? Storage,
    [property: JsonPropertyName("board")] BoardInfo? Board,
    [property: JsonPropertyName("fans")] IReadOnlyList<FanInfo>? Fans,
    [property: JsonPropertyName("voltages")] IReadOnlyList<VoltageInfo>? Voltages,
    [property: JsonPropertyName("net")] NetInfo? Net,
    [property: JsonPropertyName("battery")] BatteryInfo? Battery,
    [property: JsonPropertyName("uptime_sec")] long? UptimeSec,
    [property: JsonPropertyName("atk_fans")] IReadOnlyList<FanInfo>? AtkFans,
    [property: JsonPropertyName("display")] DisplayInfo? Display,
    [property: JsonPropertyName("displays")] IReadOnlyList<DisplayInfo>? Displays,
    [property: JsonPropertyName("health")] HealthInfo? Health);

/// <summary>Active display info — resolution and refresh rate of a monitor.</summary>
public record DisplayInfo(
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("width")] int Width,
    [property: JsonPropertyName("height")] int Height,
    [property: JsonPropertyName("refresh_hz")] int RefreshHz);

/// <summary>Service-side health summary, attached to every snapshot. <c>pawnio</c>: "ok" | "missing" | "denied". <c>degraded</c>: true if any reading is unavailable. <c>notes</c>: human-readable detail (optional).</summary>
public record HealthInfo(
    [property: JsonPropertyName("pawnio")] string Pawnio,
    [property: JsonPropertyName("degraded")] bool Degraded,
    [property: JsonPropertyName("notes")] string? Notes);

/// <summary>
/// CPU metrics. Units: <c>load</c>/<c>cores</c> = 0–100 %; <c>temp</c>,
/// <c>distance_to_tjmax_c</c> = °C; <c>clock_mhz</c> = MHz; every
/// <c>power_*_w</c> = watts; <c>voltage_v</c> = volts.
///
/// Power breakdown (Intel RAPL domains, AMD-equivalent where exposed):
/// <c>power_w</c> = whole-package draw (always the primary reading);
/// <c>power_cores_w</c>, <c>power_memory_w</c>, <c>power_platform_w</c> = the
/// RAPL sub-domains when the silicon exposes them.
///
/// Hybrid-CPU split: <c>p_core_count</c> and <c>e_core_count</c> describe how
/// the entries of <c>cores</c>/<c>core_temps</c> partition (P-Cores first,
/// then E-Cores). On non-hybrid CPUs both counts are <c>null</c> and the heat
/// map renders uniformly.
/// </summary>
public record CpuInfo(
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("load")] double? Load,
    [property: JsonPropertyName("cores")] IReadOnlyList<double>? Cores,
    [property: JsonPropertyName("temp")] double? Temp,
    [property: JsonPropertyName("clock_mhz")] double? ClockMhz,
    [property: JsonPropertyName("power_w")] double? PowerW,
    [property: JsonPropertyName("core_temps")] IReadOnlyList<double?>? CoreTemps,
    [property: JsonPropertyName("voltage_v")] double? VoltageV,
    [property: JsonPropertyName("distance_to_tjmax_c")] double? DistanceToTjMaxC,
    [property: JsonPropertyName("power_cores_w")] double? PowerCoresW,
    [property: JsonPropertyName("power_memory_w")] double? PowerMemoryW,
    [property: JsonPropertyName("power_platform_w")] double? PowerPlatformW,
    [property: JsonPropertyName("p_core_count")] int? PCoreCount,
    [property: JsonPropertyName("e_core_count")] int? ECoreCount);

/// <summary>
/// GPU metrics. <c>kind</c>: "discrete" | "integrated"; VRAM sizes in MB;
/// <c>load</c> 0–100 %; temps in °C.
///
/// New in v0.5.0: <c>memory_junction_temp_c</c> is the GDDR/VRAM die hot spot
/// (a separate physical sensor from <c>temp</c>/<c>hot_spot_temp_c</c>);
/// <c>pcie_rx_bps</c> / <c>pcie_tx_bps</c> are PCIe link-direction throughput
/// in bytes per second; <c>dedicated_vram_used_mb</c> /
/// <c>shared_vram_used_mb</c> split memory between on-card VRAM and
/// DXGI-shared system RAM (the latter is mostly meaningful on iGPUs and
/// laptops where the GPU spills into RAM); <c>voltage_v</c> is the GPU core
/// rail.
/// </summary>
public record GpuInfo(
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("kind")] string Kind,
    [property: JsonPropertyName("load")] double? Load,
    [property: JsonPropertyName("temp")] double? Temp,
    [property: JsonPropertyName("vram_used_mb")] double? VramUsedMb,
    [property: JsonPropertyName("vram_total_mb")] double? VramTotalMb,
    [property: JsonPropertyName("clock_mhz")] double? ClockMhz,
    [property: JsonPropertyName("fan_rpm")] double? FanRpm,
    [property: JsonPropertyName("power_w")] double? PowerW,
    [property: JsonPropertyName("memory_load")] double? MemoryLoad,
    [property: JsonPropertyName("hot_spot_temp")] double? HotSpotTempC,
    [property: JsonPropertyName("memory_junction_temp_c")] double? MemoryJunctionTempC,
    [property: JsonPropertyName("pcie_rx_bps")] double? PcieRxBps,
    [property: JsonPropertyName("pcie_tx_bps")] double? PcieTxBps,
    [property: JsonPropertyName("dedicated_vram_used_mb")] double? DedicatedVramUsedMb,
    [property: JsonPropertyName("shared_vram_used_mb")] double? SharedVramUsedMb,
    [property: JsonPropertyName("voltage_v")] double? VoltageV,
    [property: JsonPropertyName("d3d_engines")] IReadOnlyList<D3DEngineLoad>? D3DEngines);

/// <summary>Per-engine GPU utilisation reading (e.g. "3D", "Copy", "Video Encode"). <c>load</c> is 0-100 %.</summary>
public record D3DEngineLoad(
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("load")] double Load);

/// <summary>RAM metrics. All sizes in MB. <c>cached_mb</c> = OS file-cache (system cache pages × page size). <c>load</c> = 0–100 %. <c>dimm_temps</c> lists per-module DIMM temperatures (°C) when the SPD hub exposes them — typical on DDR5 platforms.</summary>
public record RamInfo(
    [property: JsonPropertyName("used_mb")] double? UsedMb,
    [property: JsonPropertyName("total_mb")] double? TotalMb,
    [property: JsonPropertyName("available_mb")] double? AvailableMb,
    [property: JsonPropertyName("load")] double? Load,
    [property: JsonPropertyName("cached_mb")] double? CachedMb,
    [property: JsonPropertyName("pagefile_used_mb")] double? PagefileUsedMb,
    [property: JsonPropertyName("pagefile_total_mb")] double? PagefileTotalMb,
    [property: JsonPropertyName("dimm_temps")] IReadOnlyList<DimmTemp>? DimmTemps);

/// <summary>One DIMM (memory module) temperature reading. <c>label</c> is the SPD slot label (e.g. "DIMM #0").</summary>
public record DimmTemp(
    [property: JsonPropertyName("label")] string Label,
    [property: JsonPropertyName("temp_c")] double TempC);

/// <summary>
/// Per-drive storage metrics. <c>kind</c>: "nvme" | "ssd" | "hdd"; sizes in
/// GB; <c>temp</c> = °C; <c>activity</c> = 0–100 %; <c>health</c> = 0–100 %
/// remaining life (100 = new); <c>read_bps</c> / <c>write_bps</c> =
/// bytes/second.
///
/// New in v0.5.0: <c>power_on_hours</c> (drive lifetime usage),
/// <c>power_on_count</c> (cold-start cycles), and <c>available_spare_pct</c>
/// (NVMe-spec reserved blocks remaining, 0–100 %).
/// </summary>
public record StorageInfo(
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("kind")] string Kind,
    [property: JsonPropertyName("temp")] double? Temp,
    [property: JsonPropertyName("activity")] double? Activity,
    [property: JsonPropertyName("used_gb")] double? UsedGb,
    [property: JsonPropertyName("total_gb")] double? TotalGb,
    [property: JsonPropertyName("health")] double? Health,
    [property: JsonPropertyName("read_bps")] double? ReadBps,
    [property: JsonPropertyName("write_bps")] double? WriteBps,
    [property: JsonPropertyName("power_on_hours")] long? PowerOnHours,
    [property: JsonPropertyName("power_on_count")] long? PowerOnCount,
    [property: JsonPropertyName("available_spare_pct")] double? AvailableSparePct);

/// <summary>Motherboard temperatures in °C.</summary>
public record BoardInfo(
    [property: JsonPropertyName("temp")] double? Temp,
    [property: JsonPropertyName("vrm_temp")] double? VrmTemp);

/// <summary>Individual fan reading. <c>rpm</c> = revolutions per minute.</summary>
public record FanInfo(
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("rpm")] double? Rpm);

/// <summary>Individual voltage rail reading. <c>volts</c> = volts.</summary>
public record VoltageInfo(
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("volts")] double? Volts);

/// <summary>Active network adapter metrics. <c>down_bps</c>/<c>up_bps</c> = bytes/sec; <c>link_bps</c> = bits/sec; <c>down_pct</c>/<c>up_pct</c> = 0–100 % of link capacity (null when link speed is unknown).</summary>
public record NetInfo(
    [property: JsonPropertyName("adapter")] string Adapter,
    [property: JsonPropertyName("down_bps")] double? DownBps,
    [property: JsonPropertyName("up_bps")] double? UpBps,
    [property: JsonPropertyName("link_bps")] long? LinkBps,
    [property: JsonPropertyName("down_pct")] double? DownPct,
    [property: JsonPropertyName("up_pct")] double? UpPct);

/// <summary>Laptop battery metrics. <c>charge_pct</c> = 0–100 %; <c>rate_w</c> &gt; 0 = charging, &lt; 0 = discharging; capacity in mWh; <c>time_remaining_sec</c> only meaningful while discharging.</summary>
public record BatteryInfo(
    [property: JsonPropertyName("charge_pct")] double? ChargePct,
    [property: JsonPropertyName("rate_w")] double? RateW,
    [property: JsonPropertyName("voltage_v")] double? VoltageV,
    [property: JsonPropertyName("design_capacity_mwh")] double? DesignCapacityMwh,
    [property: JsonPropertyName("full_capacity_mwh")] double? FullCapacityMwh,
    [property: JsonPropertyName("time_remaining_sec")] long? TimeRemainingSec);
