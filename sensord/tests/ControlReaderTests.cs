using Sensord.Control;
using Xunit;

public class ControlReaderTests
{
    [Fact]
    public void Parse_IntervalMs_ReturnsMessageWithInterval()
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
    public void Parse_UnusableInput_ReturnsNullInterval(string line)
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
