using LibreHardwareMonitor.Hardware;

namespace Sensord.Sensors;

/// <summary>
/// Stable fingerprint of the currently-enumerated hardware tree. Two calls
/// return the same value iff the set of hardware <c>Identifier</c>s is
/// identical, regardless of enumeration order. Used by the sensor worker
/// to detect hot-plug events without comparing full snapshots.
/// </summary>
public static class TopologySignature
{
    public static int Compute(IEnumerable<IHardware> hardware)
    {
        var ids = new List<string>();
        Walk(hardware, ids);
        if (ids.Count == 0) return 0;
        ids.Sort(StringComparer.Ordinal);
        return string.Join("|", ids).GetHashCode();
    }

    /// <summary>
    /// Returns the set of <c>Identifier.ToString()</c> values across the
    /// supplied hardware tree (including all <c>SubHardware</c>). Used by the
    /// worker to log added/removed identifiers when the topology changes.
    /// </summary>
    public static HashSet<string> IdentifierSet(IEnumerable<IHardware> hardware)
    {
        var set = new HashSet<string>(StringComparer.Ordinal);
        Walk(hardware, set);
        return set;
    }

    private static void Walk(IEnumerable<IHardware> items, List<string> acc)
    {
        foreach (var h in items)
        {
            acc.Add(h.Identifier.ToString());
            if (h.SubHardware is { Length: > 0 } subs)
                Walk(subs, acc);
        }
    }

    private static void Walk(IEnumerable<IHardware> items, HashSet<string> acc)
    {
        foreach (var h in items)
        {
            acc.Add(h.Identifier.ToString());
            if (h.SubHardware is { Length: > 0 } subs)
                Walk(subs, acc);
        }
    }
}
