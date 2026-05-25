using System.Management;
using Sensord.Model;

namespace Sensord.Sensors;

/// <summary>
/// Reads ASUS-specific sensor values through the ATK WMI bridge
/// (<c>root\wmi.AsusAtkWmi_WMNB</c>), which is registered by the ATK Package
/// driver on every ASUS gaming laptop and most ASUS desktops. The bridge
/// exposes a <c>DSTS</c> method that, given a 32-bit Device ID, returns a
/// 32-bit status word — for fan devices the low 16 bits carry the current
/// PWM/RPM reading and bit 16 marks the value as valid.
///
/// LibreHardwareMonitor does NOT cover this path (no LHM HardwareType.Fan is
/// enumerated on ASUS laptops because the EC is proprietary). This reader is
/// purely additive — it never replaces LHM fans, only supplements them.
///
/// Every call swallows exceptions and returns an empty list when ATK is not
/// present, the WMI class is missing, or any DSTS invocation fails. The
/// caller treats a null/empty list as "no ATK fan data available".
/// </summary>
public static class AtkReader
{
    /// <summary>ATK WMI namespace.</summary>
    private const string Namespace = @"root\wmi";

    /// <summary>ATK WMI class exposing DSTS / DEVS.</summary>
    private const string ClassName = "AsusAtkWmi_WMNB";

    /// <summary>
    /// Fan Device IDs documented in the open-source ASUS WMI driver
    /// (linux/drivers/platform/x86/asus-wmi.c) and confirmed via DSTS probe
    /// on ASUS TUF FX507VI. 0x00110013 is the CPU fan; 0x00110014 is the
    /// secondary (GPU) fan on machines with one.
    /// </summary>
    private static readonly (uint id, string label)[] FanDevices = new[]
    {
        (0x00110013u, "CPU Fan"),
        (0x00110014u, "GPU Fan"),
    };

    /// <summary>
    /// Returns every populated ATK fan reading, or <c>null</c> when the ATK
    /// bridge is unreachable. Never throws.
    /// </summary>
    public static List<FanInfo>? Read()
    {
        try
        {
            var scope = new ManagementScope(Namespace);
            scope.Connect();
            if (!scope.IsConnected) return null;

            // Get the (single) instance of AsusAtkWmi_WMNB.
            using var searcher = new ManagementObjectSearcher(scope, new ObjectQuery($"SELECT * FROM {ClassName}"));
            using var collection = searcher.Get();
            ManagementObject? instance = null;
            foreach (ManagementObject mo in collection)
            {
                instance = mo;
                break;
            }
            if (instance is null) return null;

            var fans = new List<FanInfo>();
            foreach (var (id, label) in FanDevices)
            {
                double? value = InvokeDsts(instance, id);
                if (value.HasValue)
                    fans.Add(new FanInfo(label, value.Value));
            }
            instance.Dispose();
            return fans.Count > 0 ? fans : null;
        }
        catch
        {
            // ATK driver not present, WMI namespace missing, or DSTS not
            // callable — caller treats as "no data".
            return null;
        }
    }

    /// <summary>
    /// Invoke DSTS(Device_ID) and return the low 16 bits when bit 16 marks
    /// the status as valid. Returns null on any failure, including the
    /// "valid bit clear" case which means the device exists but has no
    /// current reading.
    /// </summary>
    private static double? InvokeDsts(ManagementObject instance, uint deviceId)
    {
        try
        {
            using var inParams = instance.GetMethodParameters("DSTS");
            inParams["Device_ID"] = deviceId;
            using var outParams = instance.InvokeMethod("DSTS", inParams, null);
            if (outParams?["device_status"] is not uint status) return null;
            bool valid = (status & 0x00010000u) != 0u;
            if (!valid) return null;
            return (double)(status & 0xFFFFu);
        }
        catch
        {
            return null;
        }
    }
}
