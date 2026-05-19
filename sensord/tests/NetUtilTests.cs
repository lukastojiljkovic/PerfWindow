using Sensord.Sensors;
using Xunit;

public class NetUtilTests
{
    [Fact]
    public void Computes_percentage_of_link_speed()
    {
        // 12.5 MB/s on a 1 Gbit link = 100 Mbit / 1000 Mbit = 10%
        Assert.Equal(10.0, NetUtil.Utilisation(12_500_000, 1_000_000_000), precision: 3);
    }

    [Fact]
    public void Clamps_to_100()
        => Assert.Equal(100.0, NetUtil.Utilisation(1_000_000_000, 1_000_000_000));

    [Theory]
    [InlineData(0)]
    [InlineData(-1)]
    public void Returns_zero_when_link_speed_unknown(long linkBps)
        => Assert.Equal(0.0, NetUtil.Utilisation(5_000_000, linkBps));
}
