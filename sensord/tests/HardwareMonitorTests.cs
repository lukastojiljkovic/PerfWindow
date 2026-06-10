using Sensord.Sensors;
using Xunit;

namespace Sensord.Tests;

// Category bookkeeping only: constructing a real HardwareMonitor opens an LHM
// Computer (kernel driver, WMI), which is neither safe nor meaningful in CI.
public class HardwareMonitorTests
{
    [Fact]
    public void StagedCategories_follow_the_protocol_init_order()
    {
        Assert.Equal(
            new[] { "motherboard", "gpu", "storage", "network", "controller", "battery" },
            HardwareMonitor.StagedCategories);
    }

    [Fact]
    public void Staged_walk_starts_with_cpu_and_ram_done_and_everything_else_pending()
    {
        var stager = new CategoryStager(HardwareMonitor.StagedCategories);

        Assert.Equal(new[] { "cpu", "ram" }, stager.Done);
        Assert.Equal(HardwareMonitor.StagedCategories, stager.Pending);
    }

    [Fact]
    public void Staged_walk_advances_in_order_to_exhaustion()
    {
        var stager = new CategoryStager(HardwareMonitor.StagedCategories);

        var walked = new List<string>();
        while (stager.TryAdvance(out string category))
            walked.Add(category);

        Assert.Equal(HardwareMonitor.StagedCategories, walked);
        Assert.Empty(stager.Pending);
        Assert.Equal(
            new[] { "cpu", "ram", "motherboard", "gpu", "storage", "network", "controller", "battery" },
            stager.Done);
        Assert.False(stager.TryAdvance(out _));
    }

    [Fact]
    public void Completed_stager_reports_every_category_done()
    {
        var stager = CategoryStager.Completed(HardwareMonitor.StagedCategories);

        Assert.Empty(stager.Pending);
        Assert.Equal(
            new[] { "cpu", "ram", "motherboard", "gpu", "storage", "network", "controller", "battery" },
            stager.Done);
        Assert.False(stager.TryAdvance(out _));
    }
}
