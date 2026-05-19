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
            Cpu: new CpuInfo("Test CPU", 34.2, new double[] { 38.1, 55.0 }, 58.0, 4400, 52.0),
            Gpu: null, Ram: null, Storage: null, Board: null,
            Fans: null, Voltages: null, Net: null);

        string json = JsonSerializer.Serialize(snap, SensordJsonContext.Default.Snapshot);

        Assert.Contains("\"v\":1", json);
        Assert.Contains("\"ts\":1747645200", json);
        Assert.Contains("\"cpu\":", json);
        Assert.Contains("\"clock_mhz\":4400", json);
    }

    [Fact]
    public void Omits_null_sections()
    {
        var snap = new Snapshot(1, 1, null, null, null, null, null, null, null, null);
        string json = JsonSerializer.Serialize(snap, SensordJsonContext.Default.Snapshot);
        Assert.DoesNotContain("\"gpu\"", json);
        Assert.DoesNotContain("\"cpu\"", json);
    }
}
