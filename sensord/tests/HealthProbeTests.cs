using Sensord.Model;
using Sensord.Service;
using Xunit;

public class HealthProbeTests
{
    [Fact]
    public void Probe_returns_ok_when_any_cpu_temperature_is_positive()
    {
        var temps = new double?[] { 45.2, null, 47.1 };
        HealthInfo h = HealthProbe.Classify(temps, exception: null);
        Assert.Equal("ok", h.Pawnio);
        Assert.False(h.Degraded);
        Assert.Null(h.Notes);
    }

    [Fact]
    public void Probe_returns_missing_when_no_cpu_temperature_is_present()
    {
        var temps = new double?[] { null, null };
        HealthInfo h = HealthProbe.Classify(temps, exception: null);
        Assert.Equal("missing", h.Pawnio);
        Assert.True(h.Degraded);
        Assert.NotNull(h.Notes);
    }

    [Fact]
    public void Probe_returns_denied_when_exception_was_caught()
    {
        HealthInfo h = HealthProbe.Classify(temps: null, exception: new UnauthorizedAccessException("nope"));
        Assert.Equal("denied", h.Pawnio);
        Assert.True(h.Degraded);
        Assert.Contains("nope", h.Notes!);
    }

    [Fact]
    public void Probe_returns_missing_when_temps_is_null_and_no_exception()
    {
        HealthInfo h = HealthProbe.Classify(temps: null, exception: null);
        Assert.Equal("missing", h.Pawnio);
        Assert.True(h.Degraded);
    }
}
