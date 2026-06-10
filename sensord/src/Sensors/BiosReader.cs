using System.Management;

namespace Sensord.Sensors;

/// <summary>
/// BIOS identity from WMI <c>Win32_BIOS</c> (SMBIOS version string + release
/// date). The data is immutable for the lifetime of the process, so the query
/// runs exactly once and the result is cached; any failure caches
/// <c>(null, null)</c> so a broken WMI service is probed only once too.
/// </summary>
public static class BiosReader
{
    private static readonly Lazy<(string? Version, string? Date)> s_bios =
        new(Query, LazyThreadSafetyMode.ExecutionAndPublication);

    public static (string? Version, string? Date) Read() => s_bios.Value;

    private static (string?, string?) Query()
    {
        try
        {
            using var searcher = new ManagementObjectSearcher(
                "SELECT SMBIOSBIOSVersion, ReleaseDate FROM Win32_BIOS");
            using var results = searcher.Get();
            foreach (ManagementObject mo in results)
            {
                using (mo)
                {
                    string? version = mo["SMBIOSBIOSVersion"] as string;
                    string? date = ParseDmtfDate(mo["ReleaseDate"] as string);
                    return (string.IsNullOrWhiteSpace(version) ? null : version.Trim(), date);
                }
            }
            return (null, null);
        }
        catch
        {
            // WMI unavailable / access denied — identity is cosmetic, never fatal.
            return (null, null);
        }
    }

    /// <summary>
    /// Converts a WMI DMTF datetime ("yyyymmddHHMMSS.mmmmmm+UUU") to
    /// "yyyy-MM-dd". Returns null for anything that does not start with a
    /// valid 8-digit calendar date.
    /// </summary>
    internal static string? ParseDmtfDate(string? dmtf)
    {
        if (dmtf is null || dmtf.Length < 8) return null;
        if (!DateTime.TryParseExact(dmtf.Substring(0, 8), "yyyyMMdd",
                System.Globalization.CultureInfo.InvariantCulture,
                System.Globalization.DateTimeStyles.None, out DateTime date))
            return null;
        return date.ToString("yyyy-MM-dd", System.Globalization.CultureInfo.InvariantCulture);
    }
}
