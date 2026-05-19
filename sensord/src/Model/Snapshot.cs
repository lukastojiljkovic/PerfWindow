using System.Text.Json.Serialization;

namespace Sensord.Model;

/// <summary>Top-level NDJSON snapshot emitted once per poll interval on stdout.</summary>
public record Snapshot(
    [property: JsonPropertyName("v")]        int Version,
    [property: JsonPropertyName("ts")]       long Timestamp,
    [property: JsonPropertyName("cpu")]      CpuInfo? Cpu,
    [property: JsonPropertyName("gpu")]      IReadOnlyList<GpuInfo>? Gpu,
    [property: JsonPropertyName("ram")]      RamInfo? Ram,
    [property: JsonPropertyName("storage")]  IReadOnlyList<StorageInfo>? Storage,
    [property: JsonPropertyName("board")]    BoardInfo? Board,
    [property: JsonPropertyName("fans")]     IReadOnlyList<FanInfo>? Fans,
    [property: JsonPropertyName("voltages")] IReadOnlyList<VoltageInfo>? Voltages,
    [property: JsonPropertyName("net")]      NetInfo? Net);

/// <summary>CPU metrics. <c>load</c>/<c>cores</c> = 0–100 %; <c>temp</c> = °C; <c>clock_mhz</c> = MHz; <c>power_w</c> = watts.</summary>
public record CpuInfo(
    [property: JsonPropertyName("name")]      string Name,
    [property: JsonPropertyName("load")]      double? Load,
    [property: JsonPropertyName("cores")]     IReadOnlyList<double>? Cores,
    [property: JsonPropertyName("temp")]      double? Temp,
    [property: JsonPropertyName("clock_mhz")] double? ClockMhz,
    [property: JsonPropertyName("power_w")]   double? PowerW);

/// <summary>GPU metrics. <c>kind</c>: "discrete" | "integrated"; sizes in MB; <c>load</c>/<c>fan_rpm</c>/temps same units as CPU.</summary>
public record GpuInfo(
    [property: JsonPropertyName("name")]          string Name,
    [property: JsonPropertyName("kind")]          string Kind,
    [property: JsonPropertyName("load")]          double? Load,
    [property: JsonPropertyName("temp")]          double? Temp,
    [property: JsonPropertyName("vram_used_mb")]  double? VramUsedMb,
    [property: JsonPropertyName("vram_total_mb")] double? VramTotalMb,
    [property: JsonPropertyName("clock_mhz")]     double? ClockMhz,
    [property: JsonPropertyName("fan_rpm")]       double? FanRpm,
    [property: JsonPropertyName("power_w")]       double? PowerW);

/// <summary>RAM metrics. All sizes in MB. <c>cached_mb</c> = OS file-cache (system cache pages × page size). <c>load</c> = 0–100 %.</summary>
public record RamInfo(
    [property: JsonPropertyName("used_mb")]           double? UsedMb,
    [property: JsonPropertyName("total_mb")]          double? TotalMb,
    [property: JsonPropertyName("available_mb")]      double? AvailableMb,
    [property: JsonPropertyName("load")]              double? Load,
    [property: JsonPropertyName("cached_mb")]         double? CachedMb,
    [property: JsonPropertyName("pagefile_used_mb")]  double? PagefileUsedMb,
    [property: JsonPropertyName("pagefile_total_mb")] double? PagefileTotalMb);

/// <summary>Per-drive storage metrics. <c>kind</c>: "nvme" | "ssd" | "hdd"; sizes in GB; <c>temp</c> = °C; <c>activity</c> = 0–100 %.</summary>
public record StorageInfo(
    [property: JsonPropertyName("name")]     string Name,
    [property: JsonPropertyName("kind")]     string Kind,
    [property: JsonPropertyName("temp")]     double? Temp,
    [property: JsonPropertyName("activity")] double? Activity,
    [property: JsonPropertyName("used_gb")]  double? UsedGb,
    [property: JsonPropertyName("total_gb")] double? TotalGb);

/// <summary>Motherboard temperatures in °C.</summary>
public record BoardInfo(
    [property: JsonPropertyName("temp")]     double? Temp,
    [property: JsonPropertyName("vrm_temp")] double? VrmTemp);

/// <summary>Individual fan reading. <c>rpm</c> = revolutions per minute.</summary>
public record FanInfo(
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("rpm")]  double? Rpm);

/// <summary>Individual voltage rail reading. <c>volts</c> = volts.</summary>
public record VoltageInfo(
    [property: JsonPropertyName("name")]  string Name,
    [property: JsonPropertyName("volts")] double? Volts);

/// <summary>Active network adapter metrics. <c>down_bps</c>/<c>up_bps</c> = bytes/sec; <c>link_bps</c> = bits/sec; <c>down_pct</c>/<c>up_pct</c> = 0–100 % of link capacity (null when link speed is unknown).</summary>
public record NetInfo(
    [property: JsonPropertyName("adapter")]  string Adapter,
    [property: JsonPropertyName("down_bps")] double? DownBps,
    [property: JsonPropertyName("up_bps")]   double? UpBps,
    [property: JsonPropertyName("link_bps")] long? LinkBps,
    [property: JsonPropertyName("down_pct")] double? DownPct,
    [property: JsonPropertyName("up_pct")]   double? UpPct);
