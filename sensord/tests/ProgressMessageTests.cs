using System.Text.Json;
using Sensord.Model;
using Xunit;

namespace Sensord.Tests;

public class ProgressMessageTests
{
    [Fact]
    public void Serializes_the_protocol_contract_example_exactly()
    {
        var msg = new ProgressMessage(new ProgressInfo(
            Loading: "gpu",
            Done: new[] { "cpu", "ram", "motherboard" },
            Pending: new[] { "storage", "network", "controller", "battery" }));

        string json = JsonSerializer.Serialize(msg, SensordJsonContext.Default.ProgressMessage);

        Assert.Equal(
            "{\"progress\":{\"loading\":\"gpu\",\"done\":[\"cpu\",\"ram\",\"motherboard\"],\"pending\":[\"storage\",\"network\",\"controller\",\"battery\"]}}",
            json);
    }

    [Fact]
    public void Final_line_keeps_an_explicit_null_loading_and_empty_pending()
    {
        var msg = new ProgressMessage(new ProgressInfo(
            Loading: null,
            Done: new[] { "cpu", "ram", "motherboard", "gpu", "storage", "network", "controller", "battery" },
            Pending: Array.Empty<string>()));

        string json = JsonSerializer.Serialize(msg, SensordJsonContext.Default.ProgressMessage);

        Assert.Equal(
            "{\"progress\":{\"loading\":null,\"done\":[\"cpu\",\"ram\",\"motherboard\",\"gpu\",\"storage\",\"network\",\"controller\",\"battery\"],\"pending\":[]}}",
            json);
    }

    [Fact]
    public void Attach_line_reports_cpu_loading_and_nothing_done()
    {
        var msg = new ProgressMessage(new ProgressInfo(
            Loading: "cpu",
            Done: Array.Empty<string>(),
            Pending: new[] { "ram", "motherboard", "gpu", "storage", "network", "controller", "battery" }));

        string json = JsonSerializer.Serialize(msg, SensordJsonContext.Default.ProgressMessage);

        Assert.StartsWith("{\"progress\":{\"loading\":\"cpu\",\"done\":[],\"pending\":[\"ram\",", json);
    }

    [Fact]
    public void Progress_line_is_not_snapshot_shaped()
    {
        // Pre-0.10 dashboards must fail to parse this as a snapshot and drop
        // the line: it must never carry the snapshot's required fields.
        var msg = new ProgressMessage(new ProgressInfo(
            "cpu", Array.Empty<string>(), Array.Empty<string>()));

        string json = JsonSerializer.Serialize(msg, SensordJsonContext.Default.ProgressMessage);

        Assert.DoesNotContain("\"v\":", json);
        Assert.DoesNotContain("\"ts\":", json);
    }
}
