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

    [Fact]
    public void Parse_ShutdownTrue_ReturnsMessageWithShutdownTrue()
    {
        var msg = ControlReader.Parse("{\"shutdown\":true}");
        Assert.NotNull(msg);
        Assert.True(msg!.Shutdown);
    }

    [Fact]
    public void Parse_NoShutdownField_DefaultsToFalse()
    {
        var msg = ControlReader.Parse("{\"interval_ms\":500}");
        Assert.NotNull(msg);
        Assert.False(msg!.Shutdown);
    }
}
