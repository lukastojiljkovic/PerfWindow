using System.Text.Json;
using Sensord.Model;
using Xunit;

public class SnapshotSerializationTests
{
    [Fact]
    public void Serialises_with_schema_field_names()
    {
        var snap = new Snapshot(
            Version: 1, Timestamp: 1747645200,
            Cpu: new CpuInfo(
                Name: "Test CPU",
                Load: 34.2,
                Cores: new double[] { 38.1, 55.0 },
                Temp: 58.0,
                ClockMhz: 4400,
                PowerW: 52.0,
                CoreTemps: null,
                VoltageV: null,
                DistanceToTjMaxC: null,
                PowerCoresW: null,
                PowerMemoryW: null,
                PowerPlatformW: null,
                PCoreCount: null,
                ECoreCount: null),
            Gpu: null, Igpu: null, Ram: null, Storage: null, Board: null,
            Fans: null, Voltages: null, Net: null,
            Battery: null, UptimeSec: null, AtkFans: null, Display: null, Displays: null, Health: null);

        string json = JsonSerializer.Serialize(snap, SensordJsonContext.Default.Snapshot);

        Assert.Contains("\"v\":1", json);
        Assert.Contains("\"ts\":1747645200", json);
        Assert.Contains("\"cpu\":", json);
        Assert.Contains("\"clock_mhz\":4400", json);
    }

    [Fact]
    public void Omits_null_sections()
    {
        var snap = new Snapshot(
            Version: 1, Timestamp: 1, Cpu: null, Gpu: null, Igpu: null, Ram: null,
            Storage: null, Board: null, Fans: null, Voltages: null,
            Net: null, Battery: null, UptimeSec: null, AtkFans: null, Display: null, Displays: null, Health: null);
        string json = JsonSerializer.Serialize(snap, SensordJsonContext.Default.Snapshot);
        Assert.DoesNotContain("\"gpu\"", json);
        Assert.DoesNotContain("\"cpu\"", json);
        Assert.DoesNotContain("\"igpu\"", json);
    }
}
