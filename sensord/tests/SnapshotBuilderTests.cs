using Sensord.Sensors;
using Xunit;

public class SnapshotBuilderTests
{
    [Theory]
    [InlineData("/nvme/0", "nvme")]
    [InlineData("/nvme/1", "nvme")]
    [InlineData("/ssd/0",  "ssd")]
    [InlineData("/hdd/2",  "hdd")]
    [InlineData("garbage", "hdd")]  // unrecognized identifier falls back to safe default
    public void ClassifyDiskByIdentifier_returns_correct_kind(string identifier, string expected)
        => Assert.Equal(expected, SnapshotBuilder.ClassifyDiskByIdentifier(identifier));
}
