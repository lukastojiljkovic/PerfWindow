using LibreHardwareMonitor.Hardware;

namespace Sensord.Sensors;

internal static class SensorEx
{
    // NaN/Infinity from a glitched driver must never reach the snapshot:
    // System.Text.Json (strict number handling, matching serde_json on the
    // dashboard side) throws on non-finite values, which would kill EVERY
    // snapshot, not just the bad field. Non-finite readings are treated
    // exactly like null readings: skipped in favour of the next match.

    /// <summary>First sensor of <paramref name="type"/> whose name contains <paramref name="nameContains"/> and carries a finite value.</summary>
    public static double? Val(this IHardware hw, SensorType type, string nameContains)
    {
        foreach (var s in hw.Sensors)
            if (s.SensorType == type &&
                s.Name.Contains(nameContains, StringComparison.OrdinalIgnoreCase) &&
                s.Value is float v && float.IsFinite(v))
                return v;
        return null;
    }

    /// <summary>First sensor of <paramref name="type"/> with a finite value, any name.</summary>
    public static double? FirstVal(this IHardware hw, SensorType type)
    {
        foreach (var s in hw.Sensors)
            if (s.SensorType == type && s.Value is float v && float.IsFinite(v))
                return v;
        return null;
    }
}
