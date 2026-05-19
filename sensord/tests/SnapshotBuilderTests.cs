using Sensord.Model;
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

    [Fact]
    public void PreferDiscreteGpus_drops_integrated_when_a_discrete_gpu_is_present()
    {
        var gpus = new List<GpuInfo>
        {
            Gpu("NVIDIA GeForce RTX 4070 Laptop GPU", "discrete"),
            Gpu("Intel(R) UHD Graphics", "integrated"),
        };

        var result = SnapshotBuilder.PreferDiscreteGpus(gpus);

        Assert.Single(result);
        Assert.Equal("discrete", result[0].Kind);
    }

    [Fact]
    public void PreferDiscreteGpus_keeps_the_integrated_gpu_when_no_discrete_one_exists()
    {
        var gpus = new List<GpuInfo> { Gpu("Intel(R) UHD Graphics", "integrated") };

        var result = SnapshotBuilder.PreferDiscreteGpus(gpus);

        Assert.Single(result);
        Assert.Equal("integrated", result[0].Kind);
    }

    /// <summary>A GpuInfo with only the name and kind these tests care about.</summary>
    private static GpuInfo Gpu(string name, string kind)
        => new(name, kind, null, null, null, null, null, null, null);
}
