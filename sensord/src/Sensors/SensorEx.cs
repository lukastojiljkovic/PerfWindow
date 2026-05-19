using LibreHardwareMonitor.Hardware;

namespace Sensord.Sensors;

internal static class SensorEx
{
    /// <summary>First sensor of <paramref name="type"/> whose name contains <paramref name="nameContains"/>.</summary>
    public static double? Val(this IHardware hw, SensorType type, string nameContains)
    {
        foreach (var s in hw.Sensors)
            if (s.SensorType == type &&
                s.Name.Contains(nameContains, StringComparison.OrdinalIgnoreCase) &&
                s.Value is float v)
                return v;
        return null;
    }

    /// <summary>First sensor of <paramref name="type"/>, any name.</summary>
    public static double? FirstVal(this IHardware hw, SensorType type)
    {
        foreach (var s in hw.Sensors)
            if (s.SensorType == type && s.Value is float v)
                return v;
        return null;
    }
}
