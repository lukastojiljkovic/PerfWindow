using System.Management;

namespace Sensord.Sensors;

/// <summary>Per-physical-disk geometry and media type sourced from <c>MSFT_PhysicalDisk</c> WMI.</summary>
public readonly record struct PhysicalDiskInfo(string Kind, double TotalGb);

/// <summary>Reads physical-disk metadata from the Windows Storage WMI namespace once at startup.</summary>
public static class DiskInfoReader
{
    /// <summary>
    /// Returns a dictionary keyed by physical-disk index (matching the trailing integer
    /// in an LHM <c>IHardware.Identifier</c> such as <c>/nvme/0</c>).
    /// On any WMI failure the dictionary is empty — the caller must handle the absent-key case.
    /// </summary>
    public static IReadOnlyDictionary<int, PhysicalDiskInfo> Read()
    {
        var result = new Dictionary<int, PhysicalDiskInfo>();
        try
        {
            var scope = new ManagementScope(@"\\.\root\Microsoft\Windows\Storage");
            scope.Connect();

            using var searcher = new ManagementObjectSearcher(
                scope,
                new ObjectQuery("SELECT DeviceId, Size, MediaType, BusType FROM MSFT_PhysicalDisk"));

            using var collection = searcher.Get();
            foreach (ManagementBaseObject obj in collection)
            {
                using (obj)
                {
                    if (obj["DeviceId"] is not string deviceIdStr
                        || !int.TryParse(deviceIdStr, out int diskIndex))
                        continue;

                    var busType   = obj["BusType"]   is ushort bt ? bt : (ushort)0;
                    var mediaType = obj["MediaType"]  is ushort mt ? mt : (ushort)0;
                    var sizeBytes = obj["Size"]       is ulong  sz ? sz : 0UL;

                    string kind    = ClassifyDisk(busType, mediaType);
                    double totalGb = sizeBytes / 1024.0 / 1024.0 / 1024.0;

                    result[diskIndex] = new PhysicalDiskInfo(kind, totalGb);
                }
            }
        }
        catch
        {
            // WMI unavailable, access denied, timeout, etc. — return whatever we collected so far
            // (which may be empty).
        }
        return result;
    }

    /// <summary>
    /// Classifies a physical disk by its WMI BusType and MediaType codes.
    /// BusType takes priority so that NVMe drives (busType 17) are never misidentified
    /// even when they also report MediaType 4 (SSD).
    /// </summary>
    /// <param name="busType">MSFT_PhysicalDisk.BusType (17 = NVMe, 11 = SATA, 7 = USB, …)</param>
    /// <param name="mediaType">MSFT_PhysicalDisk.MediaType (3 = HDD, 4 = SSD, 0 = Unspecified)</param>
    internal static string ClassifyDisk(ushort busType, ushort mediaType)
    {
        if (busType == 17)   return "nvme";
        if (mediaType == 4)  return "ssd";
        return "hdd";
    }
}
