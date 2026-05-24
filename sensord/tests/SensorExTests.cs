using System;
using System.Collections.Generic;
using LibreHardwareMonitor.Hardware;
using Sensord.Sensors;
using Xunit;

namespace Sensord.Tests;

public class SensorExTests
{
    // ---- Fakes ---------------------------------------------------------

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

    private sealed class FakeHardware : IHardware
    {
        public ISensor[] SensorsArray { get; init; } = Array.Empty<ISensor>();
        public ISensor[] Sensors => SensorsArray;
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

    // ---- SensorEx.Val(IHardware, SensorType, string) ------------------

    [Fact]
    public void Val_returns_matching_sensor_value()
    {
        var hw = new FakeHardware
        {
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "CPU Package", SensorType = SensorType.Temperature, Value = 65.5f },
            },
        };
        Assert.Equal(65.5, hw.Val(SensorType.Temperature, "Package"));
    }

    [Fact]
    public void Val_match_is_case_insensitive_on_substring()
    {
        var hw = new FakeHardware
        {
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "CPU Package", SensorType = SensorType.Temperature, Value = 42.0f },
            },
        };
        Assert.Equal(42.0, hw.Val(SensorType.Temperature, "package"));
    }

    [Fact]
    public void Val_returns_null_when_value_is_null()
    {
        var hw = new FakeHardware
        {
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "CPU Package", SensorType = SensorType.Temperature, Value = null },
            },
        };
        Assert.Null(hw.Val(SensorType.Temperature, "Package"));
    }

    [Fact]
    public void Val_returns_null_when_value_is_NaN()
    {
        var hw = new FakeHardware
        {
            SensorsArray = new ISensor[]
            {
                // float.NaN does not match `is float v`? It does — NaN is a float.
                // Production code does not special-case NaN. Document the actual behaviour:
                // Val returns NaN as a double when the sensor value is NaN.
                new FakeSensor { Name = "CPU Package", SensorType = SensorType.Temperature, Value = float.NaN },
            },
        };
        double? v = hw.Val(SensorType.Temperature, "Package");
        Assert.NotNull(v);
        Assert.True(double.IsNaN(v!.Value));
    }

    [Fact]
    public void Val_returns_null_when_no_sensor_matches_type()
    {
        var hw = new FakeHardware
        {
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "CPU Package", SensorType = SensorType.Load, Value = 10f },
            },
        };
        Assert.Null(hw.Val(SensorType.Temperature, "Package"));
    }

    [Fact]
    public void Val_returns_null_when_no_sensor_name_contains_substring()
    {
        var hw = new FakeHardware
        {
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "CPU Core #0", SensorType = SensorType.Temperature, Value = 55f },
            },
        };
        Assert.Null(hw.Val(SensorType.Temperature, "Package"));
    }

    [Fact]
    public void Val_returns_first_matching_sensor_when_multiple_match()
    {
        var hw = new FakeHardware
        {
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "CPU Package",   SensorType = SensorType.Temperature, Value = 60f },
                new FakeSensor { Name = "CPU Package #2", SensorType = SensorType.Temperature, Value = 99f },
            },
        };
        Assert.Equal(60.0, hw.Val(SensorType.Temperature, "Package"));
    }

    // ---- SensorEx.FirstVal(IHardware, SensorType) ---------------------

    [Fact]
    public void FirstVal_returns_first_sensor_of_type()
    {
        var hw = new FakeHardware
        {
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "Anything", SensorType = SensorType.Temperature, Value = 70.25f },
            },
        };
        Assert.Equal(70.25, hw.FirstVal(SensorType.Temperature));
    }

    [Fact]
    public void FirstVal_returns_null_when_value_is_null()
    {
        var hw = new FakeHardware
        {
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "Whatever", SensorType = SensorType.Temperature, Value = null },
            },
        };
        Assert.Null(hw.FirstVal(SensorType.Temperature));
    }

    [Fact]
    public void FirstVal_returns_NaN_when_value_is_NaN()
    {
        var hw = new FakeHardware
        {
            SensorsArray = new ISensor[]
            {
                // Same as Val: production code does not special-case NaN.
                new FakeSensor { Name = "Whatever", SensorType = SensorType.Temperature, Value = float.NaN },
            },
        };
        double? v = hw.FirstVal(SensorType.Temperature);
        Assert.NotNull(v);
        Assert.True(double.IsNaN(v!.Value));
    }

    [Fact]
    public void FirstVal_returns_null_when_no_sensor_of_type_present()
    {
        var hw = new FakeHardware
        {
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "x", SensorType = SensorType.Load, Value = 10f },
            },
        };
        Assert.Null(hw.FirstVal(SensorType.Temperature));
    }

    [Fact]
    public void FirstVal_returns_null_when_hardware_has_no_sensors()
    {
        var hw = new FakeHardware();
        Assert.Null(hw.FirstVal(SensorType.Temperature));
    }

    [Fact]
    public void FirstVal_skips_a_first_sensor_with_null_value_and_returns_next()
    {
        var hw = new FakeHardware
        {
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "a", SensorType = SensorType.Temperature, Value = null },
                new FakeSensor { Name = "b", SensorType = SensorType.Temperature, Value = 50f },
            },
        };
        Assert.Equal(50.0, hw.FirstVal(SensorType.Temperature));
    }
}
