using LibreHardwareMonitor.Hardware;
using Sensord.Model;
using Sensord.Service;
using Xunit;

public class HealthProbeTests
{
    [Fact]
    public void Probe_returns_ok_when_any_cpu_temperature_is_positive()
    {
        var cpuTemps = new double?[] { 45.2, null, 47.1 };
        HealthInfo h = HealthProbe.Classify(cpuTemps, exception: null);
        Assert.Equal("ok", h.Pawnio);
        Assert.False(h.Degraded);
        Assert.Null(h.Notes);
    }

    [Fact]
    public void Probe_returns_missing_when_no_cpu_temperature_is_present()
    {
        var cpuTemps = new double?[] { null, null };
        HealthInfo h = HealthProbe.Classify(cpuTemps, exception: null);
        Assert.Equal("missing", h.Pawnio);
        Assert.True(h.Degraded);
        Assert.NotNull(h.Notes);
    }

    [Fact]
    public void Probe_returns_denied_when_exception_was_caught()
    {
        HealthInfo h = HealthProbe.Classify(cpuTemps: null, exception: new UnauthorizedAccessException("nope"));
        Assert.Equal("denied", h.Pawnio);
        Assert.True(h.Degraded);
        Assert.Contains("nope", h.Notes!);
    }

    [Fact]
    public void Probe_returns_missing_when_temps_is_null_and_no_exception()
    {
        HealthInfo h = HealthProbe.Classify(cpuTemps: null, exception: null);
        Assert.Equal("missing", h.Pawnio);
        Assert.True(h.Degraded);
    }

    // ---- SensorPipeWorker.RunProbe (SND-5: CPU temperatures only) ------

    [Fact]
    public void RunProbe_reports_ok_when_a_cpu_temperature_is_readable()
    {
        var cpu = Hw(HardwareType.Cpu, Temp(48.5f));

        HealthInfo h = SensorPipeWorker.RunProbe(new IHardware[] { cpu });

        Assert.Equal("ok", h.Pawnio);
        Assert.False(h.Degraded);
    }

    [Fact]
    public void RunProbe_ignores_non_cpu_temperatures()
    {
        // A motherboard or drive thermal comes from another chip entirely and
        // must not mask a dead PawnIO/MSR driver as "ok".
        var board = Hw(HardwareType.Motherboard, Temp(41.0f));
        var ssd = Hw(HardwareType.Storage, Temp(38.0f));

        HealthInfo h = SensorPipeWorker.RunProbe(new IHardware[] { board, ssd });

        Assert.Equal("missing", h.Pawnio);
        Assert.True(h.Degraded);
    }

    [Fact]
    public void RunProbe_reports_missing_when_cpu_temperatures_carry_no_value()
    {
        var cpu = Hw(HardwareType.Cpu, Temp(null));
        var board = Hw(HardwareType.Motherboard, Temp(44.0f));

        HealthInfo h = SensorPipeWorker.RunProbe(new IHardware[] { cpu, board });

        Assert.Equal("missing", h.Pawnio);
        Assert.True(h.Degraded);
    }

    [Fact]
    public void RunProbe_ignores_cpu_sensors_that_are_not_temperatures()
    {
        var cpu = Hw(HardwareType.Cpu,
            new FakeSensor { SensorType = SensorType.Load, Value = 12.0f },
            new FakeSensor { SensorType = SensorType.Clock, Value = 4200.0f });

        HealthInfo h = SensorPipeWorker.RunProbe(new IHardware[] { cpu });

        Assert.Equal("missing", h.Pawnio);
    }

    [Fact]
    public void RunProbe_reports_denied_when_the_hardware_tree_throws()
    {
        HealthInfo h = SensorPipeWorker.RunProbe(new IHardware[] { new ThrowingHardware() });

        Assert.Equal("denied", h.Pawnio);
        Assert.True(h.Degraded);
    }

    // ---- Fakes ----------------------------------------------------------

    private static FakeSensor Temp(float? value)
        => new() { SensorType = SensorType.Temperature, Value = value };

    private static FakeHardware Hw(HardwareType type, params ISensor[] sensors)
        => new() { HardwareType = type, SensorsArray = sensors };

    private sealed class FakeSensor : ISensor
    {
        public float? Value { get; set; }
        public string Name { get; set; } = "fake";
        public int Index { get; set; }
        public SensorType SensorType { get; set; } = SensorType.Temperature;
        public Identifier Identifier => new("fake");
        public bool IsDefaultHidden { get; set; }
        public IControl Control => null!;
        public IHardware Hardware => null!;
        public float? Min { get; set; }
        public float? Max { get; set; }
        public IEnumerable<SensorValue> Values => Array.Empty<SensorValue>();
        public IReadOnlyList<IParameter> Parameters => Array.Empty<IParameter>();
        public TimeSpan ValuesTimeWindow { get; set; }
        public void Accept(IVisitor visitor) { }
        public void Traverse(IVisitor visitor) { }
        public void ResetMin() { }
        public void ResetMax() { }
        public void ClearValues() { }
    }

    private class FakeHardware : IHardware
    {
        public ISensor[] SensorsArray { get; init; } = Array.Empty<ISensor>();
        public virtual ISensor[] Sensors => SensorsArray;
        public string Name { get; set; } = "fake-hw";
        public Identifier Identifier => new("fake-hw");
        public HardwareType HardwareType { get; set; } = HardwareType.Cpu;
        public IHardware Parent => null!;
        public IHardware[] SubHardware => Array.Empty<IHardware>();
        public IDictionary<string, string> Properties => new Dictionary<string, string>();
        public string GetReport() => string.Empty;
        public void Update() { }
        public void Accept(IVisitor visitor) { }
        public void Traverse(IVisitor visitor) { }
        public event SensorEventHandler? SensorAdded { add { } remove { } }
        public event SensorEventHandler? SensorRemoved { add { } remove { } }
    }

    private sealed class ThrowingHardware : FakeHardware
    {
        public override ISensor[] Sensors => throw new UnauthorizedAccessException("driver handle denied");
    }
}
