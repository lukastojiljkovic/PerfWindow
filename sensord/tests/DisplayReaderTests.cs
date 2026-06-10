using Sensord.Sensors;
using Xunit;

namespace Sensord.Tests;

public class DisplayReaderTests
{
    [Fact]
    public void Read_ReturnsFirstEntryFromReadAll()
    {
        // The single-display compat path delegates to ReadAll().
        // On a real machine ReadAll returns >=1 monitor; ensure Read picks the same one.
        var all = DisplayReader.ReadAll();
        var primary = DisplayReader.Read();
        if (all.Count == 0)
        {
            Assert.Null(primary);
        }
        else
        {
            Assert.NotNull(primary);
            Assert.Equal(all[0].Name, primary!.Name);
            Assert.Equal(all[0].Width, primary.Width);
            Assert.Equal(all[0].Height, primary.Height);
            Assert.Equal(all[0].RefreshHz, primary.RefreshHz);
        }
    }

    [Fact]
    public void ReadAll_ReturnsNonNullList()
    {
        var all = DisplayReader.ReadAll();
        Assert.NotNull(all);
    }

    [Theory]
    [InlineData(null, null)]
    [InlineData("", null)]
    [InlineData("   ", null)]
    [InlineData("ROG XG27AQ", "ROG XG27AQ")]
    [InlineData("  Dell U2723QE ", "Dell U2723QE")]
    public void CleanModel_normalises_edid_friendly_names(string? input, string? expected)
        => Assert.Equal(expected, DisplayReader.CleanModel(input));
}
