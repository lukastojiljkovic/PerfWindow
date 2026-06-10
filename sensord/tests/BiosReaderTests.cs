using Sensord.Sensors;
using Xunit;

namespace Sensord.Tests;

public class BiosReaderTests
{
    [Theory]
    [InlineData("20231115000000.000000+000", "2023-11-15")]
    [InlineData("20240229120000.000000+060", "2024-02-29")]
    [InlineData("19991231", "1999-12-31")]
    public void ParseDmtfDate_converts_valid_dmtf_to_iso_date(string input, string expected)
        => Assert.Equal(expected, BiosReader.ParseDmtfDate(input));

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("2023")]
    [InlineData("garbage!")]
    [InlineData("abcdefgh0000.000000+000")]
    [InlineData("20231340000000.000000+000")]
    public void ParseDmtfDate_returns_null_for_invalid_input(string? input)
        => Assert.Null(BiosReader.ParseDmtfDate(input));

    [Fact]
    public void Read_never_throws_and_returns_a_stable_cached_result()
    {
        var first = BiosReader.Read();
        var second = BiosReader.Read();
        Assert.Equal(first, second);
    }
}
