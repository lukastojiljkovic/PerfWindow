using Sensord.Service;
using Xunit;

namespace Sensord.Tests;

public class TopologyDebounceTests
{
    private static HashSet<string> Ids(params string[] ids) => new(ids, StringComparer.Ordinal);

    [Fact]
    public void Unchanged_set_never_triggers_a_rebuild()
    {
        var debounce = new TopologyDebounce(Ids("/cpu/0", "/gpu/0"));

        for (int i = 0; i < 10; i++)
            Assert.False(debounce.ShouldRebuild(Ids("/cpu/0", "/gpu/0")));
    }

    [Fact]
    public void Single_transient_change_does_not_trigger_a_rebuild()
    {
        var debounce = new TopologyDebounce(Ids("/cpu/0", "/gpu/0"));

        Assert.False(debounce.ShouldRebuild(Ids("/cpu/0")));
        Assert.False(debounce.ShouldRebuild(Ids("/cpu/0", "/gpu/0")));
        // The reverted glitch must not have armed the trigger.
        Assert.False(debounce.ShouldRebuild(Ids("/cpu/0")));
    }

    [Fact]
    public void Same_change_on_two_consecutive_checks_triggers_a_rebuild()
    {
        var debounce = new TopologyDebounce(Ids("/cpu/0", "/gpu/0"));

        Assert.False(debounce.ShouldRebuild(Ids("/cpu/0")));
        Assert.True(debounce.ShouldRebuild(Ids("/cpu/0")));
    }

    [Fact]
    public void A_still_churning_set_waits_for_two_matching_sightings()
    {
        var debounce = new TopologyDebounce(Ids("/cpu/0"));

        Assert.False(debounce.ShouldRebuild(Ids("/cpu/0", "/hdd/0")));
        Assert.False(debounce.ShouldRebuild(Ids("/cpu/0", "/hdd/1")));
        Assert.True(debounce.ShouldRebuild(Ids("/cpu/0", "/hdd/1")));
    }

    [Fact]
    public void Rebaseline_resets_both_the_baseline_and_the_armed_candidate()
    {
        var debounce = new TopologyDebounce(Ids("/cpu/0"));
        Assert.False(debounce.ShouldRebuild(Ids("/cpu/0", "/hdd/0")));
        Assert.True(debounce.ShouldRebuild(Ids("/cpu/0", "/hdd/0")));

        debounce.Rebaseline(Ids("/cpu/0", "/hdd/0"));

        Assert.False(debounce.ShouldRebuild(Ids("/cpu/0", "/hdd/0")));
        Assert.False(debounce.ShouldRebuild(Ids("/cpu/0")));
        Assert.True(debounce.ShouldRebuild(Ids("/cpu/0")));
    }

    [Fact]
    public void Failed_rebuild_rearms_and_fires_again_after_two_more_sightings()
    {
        // The worker keeps the old baseline when monitor reconstruction fails;
        // a persistent change must therefore re-fire later.
        var debounce = new TopologyDebounce(Ids("/cpu/0"));
        Assert.False(debounce.ShouldRebuild(Ids("/cpu/0", "/hdd/0")));
        Assert.True(debounce.ShouldRebuild(Ids("/cpu/0", "/hdd/0")));

        Assert.False(debounce.ShouldRebuild(Ids("/cpu/0", "/hdd/0")));
        Assert.True(debounce.ShouldRebuild(Ids("/cpu/0", "/hdd/0")));
    }

    [Fact]
    public void Baseline_reflects_the_constructor_then_the_last_rebaseline()
    {
        var debounce = new TopologyDebounce(Ids("/cpu/0"));
        Assert.Equal(new[] { "/cpu/0" }, debounce.Baseline.OrderBy(x => x, StringComparer.Ordinal));

        debounce.Rebaseline(Ids("/cpu/0", "/gpu/0"));
        Assert.Equal(
            new[] { "/cpu/0", "/gpu/0" },
            debounce.Baseline.OrderBy(x => x, StringComparer.Ordinal));
    }
}
