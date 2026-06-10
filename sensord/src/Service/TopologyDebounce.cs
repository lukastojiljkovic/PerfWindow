namespace Sensord.Service;

/// <summary>
/// Two-strike filter for topology rebuilds (SND-7): a changed hardware
/// identifier set must be observed on two consecutive checks — and agree with
/// itself — before the worker tears down the monitor. One transient
/// enumeration glitch must not trigger a full Computer.Open + WMI rescan.
/// </summary>
internal sealed class TopologyDebounce
{
    private HashSet<string> _baseline;
    private HashSet<string>? _candidate;

    public TopologyDebounce(HashSet<string> baseline) => _baseline = baseline;

    /// <summary>The identifier set the running monitor was built from — kept for change logging.</summary>
    public IReadOnlyCollection<string> Baseline => _baseline;

    /// <summary>
    /// True when <paramref name="current"/> differs from the baseline and
    /// matches the previous check's sighting. Firing disarms the candidate so
    /// a failed rebuild (caller keeps the old baseline) re-arms from scratch.
    /// </summary>
    public bool ShouldRebuild(HashSet<string> current)
    {
        if (current.SetEquals(_baseline))
        {
            _candidate = null;
            return false;
        }
        if (_candidate is not null && _candidate.SetEquals(current))
        {
            _candidate = null;
            return true;
        }
        _candidate = current;
        return false;
    }

    public void Rebaseline(HashSet<string> baseline)
    {
        _baseline = baseline;
        _candidate = null;
    }
}
