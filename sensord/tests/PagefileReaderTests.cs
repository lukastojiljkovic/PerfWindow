using Sensord.Sensors;
using Xunit;

public class PagefileReaderTests
{
    [Fact]
    public void Returns_sane_values()
    {
        PagefileInfo pf = PagefileReader.Read();
        Assert.True(pf.TotalMb >= 0);
        Assert.True(pf.UsedMb >= 0);
        Assert.True(pf.UsedMb <= pf.TotalMb + 1);   // +1 tolerates rounding
    }
}
