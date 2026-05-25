using System;
using System.Collections.Generic;
using LibreHardwareMonitor.Hardware;
using Sensord.Model;
using Sensord.Sensors;
using Xunit;

public class SnapshotBuilderTests
{
    [Theory]
    [InlineData("/nvme/0", "nvme")]
    [InlineData("/nvme/1", "nvme")]
    [InlineData("/ssd/0", "ssd")]
    [InlineData("/hdd/2", "hdd")]
    [InlineData("garbage", "hdd")]  // unrecognized identifier falls back to safe default
    public void ClassifyDiskByIdentifier_returns_correct_kind(string identifier, string expected)
        => Assert.Equal(expected, SnapshotBuilder.ClassifyDiskByIdentifier(identifier));

    [Fact]
    public void PreferDiscreteGpus_drops_integrated_when_a_discrete_gpu_is_present()
    {
        var gpus = new List<GpuInfo>
        {
            Gpu("NVIDIA GeForce RTX 4070 Laptop GPU", "discrete"),
            Gpu("Intel(R) UHD Graphics", "integrated"),
        };

        var result = SnapshotBuilder.PreferDiscreteGpus(gpus);

        Assert.Single(result);
        Assert.Equal("discrete", result[0].Kind);
    }

    [Fact]
    public void PreferDiscreteGpus_keeps_the_integrated_gpu_when_no_discrete_one_exists()
    {
        var gpus = new List<GpuInfo> { Gpu("Intel(R) UHD Graphics", "integrated") };

        var result = SnapshotBuilder.PreferDiscreteGpus(gpus);

        Assert.Single(result);
        Assert.Equal("integrated", result[0].Kind);
    }

    /// <summary>A GpuInfo with only the name and kind these tests care about.</summary>
    private static GpuInfo Gpu(string name, string kind)
        => new(name, kind, null, null, null, null, null, null, null, null, null,
               null, null, null, null, null, null);

    // ---- Edge-case Build() tests ---------------------------------------

    [Fact]
    public void Build_handles_cpu_hardware_with_zero_sensors()
    {
        var cpu = new FakeHardware
        {
            Name = "Empty CPU",
            HardwareTypeValue = HardwareType.Cpu,
            SensorsArray = Array.Empty<ISensor>(),
        };
        var snap = SnapshotBuilder.Build(
            new IHardware[] { cpu },
            default,
            new Dictionary<string, long>(),
            new Dictionary<int, PhysicalDiskInfo>());

        Assert.NotNull(snap.Cpu);
        Assert.Equal("Empty CPU", snap.Cpu!.Name);
        // No sensors → all sensor-derived fields are null.
        Assert.Null(snap.Cpu.Load);
        Assert.Null(snap.Cpu.Cores);
        Assert.Null(snap.Cpu.Temp);
        Assert.Null(snap.Cpu.ClockMhz);
        Assert.Null(snap.Cpu.PowerW);
        Assert.Null(snap.Cpu.CoreTemps);
    }

    [Fact]
    public void Build_returns_null_cpu_when_no_cpu_hardware_present()
    {
        var snap = SnapshotBuilder.Build(
            Array.Empty<IHardware>(),
            default,
            new Dictionary<string, long>(),
            new Dictionary<int, PhysicalDiskInfo>());

        Assert.Null(snap.Cpu);
    }

    [Fact]
    public void Build_falls_back_to_identifier_kind_when_diskInfo_lookup_misses()
    {
        // Storage hardware with identifier "/nvme/9" but diskInfo has no entry
        // for disk 9 → kind comes from identifier prefix, capacity stays null.
        var disk = new FakeStorage
        {
            Name = "Mystery NVMe",
            IdentifierValue = "/nvme/9",
            SensorsArray = Array.Empty<ISensor>(),
        };
        var snap = SnapshotBuilder.Build(
            new IHardware[] { disk },
            default,
            new Dictionary<string, long>(),
            new Dictionary<int, PhysicalDiskInfo>());   // empty -> miss

        Assert.NotNull(snap.Storage);
        Assert.Single(snap.Storage!);
        Assert.Equal("Mystery NVMe", snap.Storage![0].Name);
        Assert.Equal("nvme", snap.Storage![0].Kind);
        Assert.Null(snap.Storage![0].TotalGb);
        Assert.Null(snap.Storage![0].UsedGb);
    }

    [Fact]
    public void Build_reads_storage_health_from_remaining_life_level_sensor()
    {
        var disk = new FakeStorage
        {
            Name = "Samsung 980 Pro",
            IdentifierValue = "/nvme/0",
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "Remaining Life", SensorType = SensorType.Level, Value = 96f },
            },
        };
        var snap = SnapshotBuilder.Build(
            new IHardware[] { disk },
            default,
            new Dictionary<string, long>(),
            new Dictionary<int, PhysicalDiskInfo>());

        Assert.Equal(96.0, snap.Storage![0].Health);
    }

    [Fact]
    public void Build_falls_back_to_available_spare_when_remaining_life_missing()
    {
        var disk = new FakeStorage
        {
            Name = "Generic NVMe",
            IdentifierValue = "/nvme/0",
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "Available Spare", SensorType = SensorType.Level, Value = 88f },
            },
        };
        var snap = SnapshotBuilder.Build(
            new IHardware[] { disk },
            default,
            new Dictionary<string, long>(),
            new Dictionary<int, PhysicalDiskInfo>());

        Assert.Equal(88.0, snap.Storage![0].Health);
    }

    [Fact]
    public void Build_leaves_storage_health_null_when_no_matching_level_sensor()
    {
        var disk = new FakeStorage
        {
            Name = "Old HDD",
            IdentifierValue = "/hdd/0",
            SensorsArray = new ISensor[]
            {
                // A Level sensor that is not health-related, plus an unrelated
                // Temperature sensor — neither should be picked up as health.
                new FakeSensor { Name = "Write Amplification", SensorType = SensorType.Level, Value = 1.4f },
                new FakeSensor { Name = "Temperature 1",       SensorType = SensorType.Temperature, Value = 38f },
            },
        };
        var snap = SnapshotBuilder.Build(
            new IHardware[] { disk },
            default,
            new Dictionary<string, long>(),
            new Dictionary<int, PhysicalDiskInfo>());

        Assert.Null(snap.Storage![0].Health);
    }

    [Fact]
    public void Build_reads_storage_read_and_write_throughput()
    {
        var disk = new FakeStorage
        {
            Name = "WD Black",
            IdentifierValue = "/nvme/0",
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "Read Rate",  SensorType = SensorType.Throughput, Value = 50_000_000f },
                new FakeSensor { Name = "Write Rate", SensorType = SensorType.Throughput, Value =  2_000_000f },
            },
        };
        var snap = SnapshotBuilder.Build(
            new IHardware[] { disk },
            default,
            new Dictionary<string, long>(),
            new Dictionary<int, PhysicalDiskInfo>());

        Assert.Equal(50_000_000.0, snap.Storage![0].ReadBps);
        Assert.Equal(2_000_000.0, snap.Storage![0].WriteBps);
    }

    [Fact]
    public void BuildCpu_reads_vcore_from_voltage_sensor()
    {
        var cpu = new FakeHardware
        {
            Name = "Test CPU",
            HardwareTypeValue = HardwareType.Cpu,
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "Vcore", SensorType = SensorType.Voltage, Value = 1.234f },
            },
        };

        var info = SnapshotBuilder.BuildCpu(cpu);
        Assert.Equal(1.234, info.VoltageV!.Value, 3);
    }

    [Fact]
    public void BuildGpus_reads_hot_spot_temperature()
    {
        var gpu = new FakeHardware
        {
            Name = "NVIDIA GeForce RTX 4070 Laptop GPU",
            HardwareTypeValue = HardwareType.GpuNvidia,
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "GPU Hot Spot", SensorType = SensorType.Temperature, Value = 85f },
            },
        };

        var gpus = SnapshotBuilder.BuildGpus(new IHardware[] { gpu });
        Assert.Single(gpus);
        Assert.Equal(85.0, gpus[0].HotSpotTempC);
    }

    [Fact]
    public void BuildBattery_collapses_discharge_rate_to_negative_rate_w()
    {
        var batt = new FakeHardware
        {
            Name = "Battery",
            HardwareTypeValue = HardwareType.Battery,
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "Charge Level",   SensorType = SensorType.Level,   Value = 64f },
                new FakeSensor { Name = "Discharge Rate", SensorType = SensorType.Power,   Value = 12.4f },
                new FakeSensor { Name = "Voltage",        SensorType = SensorType.Voltage, Value = 11.2f },
            },
        };

        var info = SnapshotBuilder.BuildBattery(new IHardware[] { batt });
        Assert.NotNull(info);
        Assert.Equal(64.0, info!.ChargePct);
        Assert.Equal(-12.4, info.RateW!.Value, 2);
        Assert.Equal(11.2, info.VoltageV!.Value, 2);
    }

    [Fact]
    public void BuildBattery_returns_null_when_no_battery_hardware_present()
    {
        var info = SnapshotBuilder.BuildBattery(Array.Empty<IHardware>());
        Assert.Null(info);
    }

    [Fact]
    public void Build_populates_uptime_seconds()
    {
        var snap = SnapshotBuilder.Build(
            Array.Empty<IHardware>(),
            default,
            new Dictionary<string, long>(),
            new Dictionary<int, PhysicalDiskInfo>());
        Assert.NotNull(snap.UptimeSec);
        Assert.True(snap.UptimeSec!.Value > 0);
    }

    [Fact]
    public void Build_uses_link_speed_lookup_by_network_adapter_name()
    {
        // One NIC named "Wi-Fi" with a positive throughput so it wins the
        // "active adapter" pick, plus a link-speed entry for that exact name.
        var nic = new FakeHardware
        {
            Name = "Wi-Fi",
            HardwareTypeValue = HardwareType.Network,
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "Download Speed", SensorType = SensorType.Throughput, Value = 1000f },
                new FakeSensor { Name = "Upload Speed",   SensorType = SensorType.Throughput, Value =  500f },
            },
        };
        var links = new Dictionary<string, long> { ["Wi-Fi"] = 1_200_000_000L };

        var snap = SnapshotBuilder.Build(
            new IHardware[] { nic },
            default,
            links,
            new Dictionary<int, PhysicalDiskInfo>());

        Assert.NotNull(snap.Net);
        Assert.Equal("Wi-Fi", snap.Net!.Adapter);
        Assert.Equal(1_200_000_000L, snap.Net.LinkBps);
    }

    // ---- Test fakes ----------------------------------------------------

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
        public virtual Identifier Identifier => new("fake-hw");
        public HardwareType HardwareTypeValue { get; set; } = HardwareType.Cpu;
        public HardwareType HardwareType => HardwareTypeValue;
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

    /// <summary>
    /// A storage IHardware whose Identifier is parsed by SnapshotBuilder.BuildStorage
    /// (which calls <c>hw.Identifier.ToString()</c>). HardwareType is fixed to Storage.
    /// </summary>
    private sealed class FakeStorage : FakeHardware
    {
        public string IdentifierValue { get; set; } = "/nvme/0";
        public override Identifier Identifier
        {
            get
            {
                // Identifier(params string[]) joins segments with '/' and prefixes '/'.
                // Splitting our value reproduces the SnapshotBuilder-visible string.
                string[] parts = IdentifierValue.TrimStart('/').Split('/');
                return new Identifier(parts);
            }
        }

        public FakeStorage()
        {
            HardwareTypeValue = HardwareType.Storage;
        }
    }
}
