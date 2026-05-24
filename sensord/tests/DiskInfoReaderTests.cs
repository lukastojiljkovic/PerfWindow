using Sensord.Sensors;
using Xunit;

public class DiskInfoReaderTests
{
    [Theory]
    [InlineData(17, 0, "nvme")]   // NVMe bus type always wins
    [InlineData(17, 4, "nvme")]   // NVMe even when MediaType reports SSD
    [InlineData(11, 4, "ssd")]    // SATA SSD (SATA bus + SSD media)
    [InlineData(11, 3, "hdd")]    // SATA HDD
    [InlineData(7, 0, "hdd")]    // USB drive, unspecified media type
    public void ClassifyDisk_returns_correct_kind(ushort busType, ushort mediaType, string expected)
        => Assert.Equal(expected, DiskInfoReader.ClassifyDisk(busType, mediaType));
}
