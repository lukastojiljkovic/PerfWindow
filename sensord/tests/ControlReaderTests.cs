using Sensord.Control;
using Xunit;

public class ControlReaderTests
{
    [Fact]
    public void Parses_interval()
    {
        ControlMessage? msg = ControlReader.Parse("{\"interval_ms\":2000}");
        Assert.NotNull(msg);
        Assert.Equal(2000, msg!.IntervalMs);
    }

    [Theory]
    [InlineData("")]
    [InlineData("not json")]
    [InlineData("{}")]
    [InlineData("{\"interval_ms\":\"x\"}")]
    public void Returns_null_for_unusable_input(string line)
    {
        Assert.Null(ControlReader.Parse(line)?.IntervalMs);
    }
}
