using System.Text.Json.Serialization;

namespace Sensord.Model;

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

public record CpuInfo(
    [property: JsonPropertyName("name")]      string Name,
    [property: JsonPropertyName("load")]      double? Load,
    [property: JsonPropertyName("cores")]     IReadOnlyList<double>? Cores,
    [property: JsonPropertyName("temp")]      double? Temp,
    [property: JsonPropertyName("clock_mhz")] double? ClockMhz,
    [property: JsonPropertyName("power_w")]   double? PowerW);

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

public record RamInfo(
    [property: JsonPropertyName("used_mb")]           double? UsedMb,
    [property: JsonPropertyName("total_mb")]          double? TotalMb,
    [property: JsonPropertyName("available_mb")]      double? AvailableMb,
    [property: JsonPropertyName("load")]              double? Load,
    [property: JsonPropertyName("cached_mb")]         double? CachedMb,
    [property: JsonPropertyName("pagefile_used_mb")]  double? PagefileUsedMb,
    [property: JsonPropertyName("pagefile_total_mb")] double? PagefileTotalMb);

public record StorageInfo(
    [property: JsonPropertyName("name")]     string Name,
    [property: JsonPropertyName("kind")]     string Kind,
    [property: JsonPropertyName("temp")]     double? Temp,
    [property: JsonPropertyName("activity")] double? Activity,
    [property: JsonPropertyName("used_gb")]  double? UsedGb,
    [property: JsonPropertyName("total_gb")] double? TotalGb);

public record BoardInfo(
    [property: JsonPropertyName("temp")]     double? Temp,
    [property: JsonPropertyName("vrm_temp")] double? VrmTemp);

public record FanInfo(
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("rpm")]  double? Rpm);

public record VoltageInfo(
    [property: JsonPropertyName("name")]  string Name,
    [property: JsonPropertyName("volts")] double? Volts);

public record NetInfo(
    [property: JsonPropertyName("adapter")]  string Adapter,
    [property: JsonPropertyName("down_bps")] double? DownBps,
    [property: JsonPropertyName("up_bps")]   double? UpBps,
    [property: JsonPropertyName("link_bps")] long? LinkBps,
    [property: JsonPropertyName("down_pct")] double? DownPct,
    [property: JsonPropertyName("up_pct")]   double? UpPct);
