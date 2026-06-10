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
               null, null, null, null, null, null, null);

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
    public void Build_HardwareWithThrowingSensor_OmitsSectionNotAborts()
    {
        // A CPU-typed hardware whose Sensors access throws. The whole snapshot
        // must still build; the CPU section becomes null. This protects the
        // worker loop from losing a whole tick when one vendor driver / LHM
        // probe glitches mid-poll.
        var throwingHw = new ThrowingFakeHardware(HardwareType.Cpu);

        var snap = SnapshotBuilder.Build(
            new IHardware[] { throwingHw },
            default,
            new Dictionary<string, long>(),
            new Dictionary<int, PhysicalDiskInfo>());

        Assert.NotNull(snap);
        Assert.Null(snap.Cpu);
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

    // ---- v0.10.0: non-finite sweep, ts_ms, new fields -------------------

    [Fact]
    public void Build_with_non_finite_sensor_values_serializes_cleanly()
    {
        // One NaN and one +Infinity reading scattered across every section
        // that historically read s.Value directly: the snapshot must build,
        // serialize (strict number handling), and carry no non-finite value.
        var cpu = new FakeHardware
        {
            Name = "Glitchy CPU",
            HardwareTypeValue = HardwareType.Cpu,
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "CPU Total",   SensorType = SensorType.Load,        Value = float.NaN },
                new FakeSensor { Name = "CPU Package", SensorType = SensorType.Temperature, Value = float.PositiveInfinity },
                new FakeSensor { Name = "CPU Core #1", SensorType = SensorType.Clock,       Value = float.NaN },
            },
        };
        var gpu = new FakeHardware
        {
            Name = "Glitchy GPU",
            HardwareTypeValue = HardwareType.GpuNvidia,
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "D3D 3D",     SensorType = SensorType.Load,  Value = float.NaN },
                new FakeSensor { Name = "GPU Memory", SensorType = SensorType.Clock, Value = float.PositiveInfinity },
            },
        };
        var io = new FakeHardware
        {
            Name = "Super IO",
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "Fan #1", SensorType = SensorType.Fan,     Value = float.NaN },
                new FakeSensor { Name = "+12V",   SensorType = SensorType.Voltage, Value = float.PositiveInfinity },
            },
        };
        var board = new FakeHardware
        {
            Name = "Board",
            HardwareTypeValue = HardwareType.Motherboard,
            SubHardwareArray = new IHardware[] { io },
        };
        var dimm = new FakeHardware
        {
            Name = "DIMM module",
            HardwareTypeValue = HardwareType.Memory,
            IdentifierValue = "/memory/dimm/0",
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "DIMM #0", SensorType = SensorType.Temperature, Value = float.NaN },
            },
        };
        var batt = new FakeHardware
        {
            Name = "Battery",
            HardwareTypeValue = HardwareType.Battery,
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "Discharge Rate", SensorType = SensorType.Power, Value = float.PositiveInfinity },
            },
        };

        var snap = SnapshotBuilder.Build(
            new IHardware[] { cpu, gpu, board, dimm, batt },
            default,
            new Dictionary<string, long>(),
            new Dictionary<int, PhysicalDiskInfo>());

        string json = System.Text.Json.JsonSerializer.Serialize(snap, SensordJsonContext.Default.Snapshot);

        Assert.DoesNotContain("NaN", json);
        Assert.DoesNotContain("Infinity", json);
        Assert.Null(snap.Cpu!.Load);
        Assert.Null(snap.Cpu.Temp);
        Assert.Equal(new double?[] { null }, snap.Cpu.CoreClocksMhz);
        Assert.Null(snap.Gpu![0].D3DEngines);
        Assert.Null(snap.Gpu[0].MemoryClockMhz);
        Assert.Null(snap.Fans);
        Assert.Null(snap.Voltages);
        Assert.Null(snap.Ram!.DimmTemps);
        Assert.Null(snap.Battery!.RateW);
    }

    [Fact]
    public void Build_emits_ts_and_ts_ms_from_the_same_instant()
    {
        var snap = SnapshotBuilder.Build(
            Array.Empty<IHardware>(),
            default,
            new Dictionary<string, long>(),
            new Dictionary<int, PhysicalDiskInfo>());

        Assert.NotNull(snap.TsMs);
        Assert.Equal(snap.Timestamp, snap.TsMs!.Value / 1000);

        string json = System.Text.Json.JsonSerializer.Serialize(snap, SensordJsonContext.Default.Snapshot);
        Assert.Contains("\"ts\":", json);
        Assert.Contains("\"ts_ms\":", json);
    }

    [Fact]
    public void BuildGpus_reads_memory_clock_and_video_engine_load()
    {
        var gpu = new FakeHardware
        {
            Name = "NVIDIA GeForce RTX 4070 Laptop GPU",
            HardwareTypeValue = HardwareType.GpuNvidia,
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "GPU Memory",       SensorType = SensorType.Clock, Value = 8001f },
                new FakeSensor { Name = "GPU Core",         SensorType = SensorType.Clock, Value = 2310f },
                new FakeSensor { Name = "GPU Video Engine", SensorType = SensorType.Load,  Value = 37.5f },
            },
        };

        var gpus = SnapshotBuilder.BuildGpus(new IHardware[] { gpu });

        Assert.Equal(8001.0, gpus[0].MemoryClockMhz);
        Assert.Equal(37.5, gpus[0].VideoEngineLoad);
        Assert.Equal(2310.0, gpus[0].ClockMhz);
    }

    [Fact]
    public void CollectRamModules_formats_full_timing_set()
    {
        // Real Kingston KF556S40 SPD numbers from the dev-machine probe dump.
        var dimm = new FakeHardware
        {
            Name = "Kingston KF556S40-16 #0",
            HardwareTypeValue = HardwareType.Memory,
            IdentifierValue = "/memory/dimm/0",
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "Capacity", SensorType = SensorType.Data,        Value = 16f },
                new FakeSensor { Name = "DIMM #0",  SensorType = SensorType.Temperature, Value = 41.5f },
                new FakeSensor { Name = "Maximum Operating Temperature", SensorType = SensorType.Temperature, Value = 85f },
                new FakeSensor { Name = "tAA (CAS Latency Time)",                SensorType = SensorType.Timing, Value = 14.28f },
                new FakeSensor { Name = "tRCD (RAS to CAS Delay Time)",          SensorType = SensorType.Timing, Value = 14.28f },
                new FakeSensor { Name = "tRP (Row Precharge Delay Time)",        SensorType = SensorType.Timing, Value = 14.28f },
                new FakeSensor { Name = "tRAS (Active to Precharge Delay Time)", SensorType = SensorType.Timing, Value = 28.56f },
                new FakeSensor { Name = "tCKAVGmax (Maximum Cycle Time)",        SensorType = SensorType.Timing, Value = 1.25f },
                new FakeSensor { Name = "tCKAVGmin (Minimum Cycle Time)",        SensorType = SensorType.Timing, Value = 0.357f },
            },
        };

        var modules = SnapshotBuilder.CollectRamModules(new IHardware[] { dimm });

        Assert.NotNull(modules);
        var m = Assert.Single(modules!);
        Assert.Equal("Kingston KF556S40-16 #0", m.Label);
        Assert.Equal(16.0, m.CapacityGb);
        Assert.Equal(41.5, m.TempC);
        Assert.Equal("CL40-40-40-80 @ 5602 MT/s", m.Timings);
    }

    [Fact]
    public void CollectRamModules_returns_null_timings_when_set_incomplete()
    {
        // Module 0 lacks tRAS entirely; module 1 has it but tCKAVGmin is NaN.
        // Both must degrade to null timings while keeping the rest.
        var dimm0 = new FakeHardware
        {
            Name = "Module A",
            HardwareTypeValue = HardwareType.Memory,
            IdentifierValue = "/memory/dimm/0",
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "Capacity", SensorType = SensorType.Data, Value = 16f },
                new FakeSensor { Name = "tAA (CAS Latency Time)",           SensorType = SensorType.Timing, Value = 14.28f },
                new FakeSensor { Name = "tRCD (RAS to CAS Delay Time)",     SensorType = SensorType.Timing, Value = 14.28f },
                new FakeSensor { Name = "tRP (Row Precharge Delay Time)",   SensorType = SensorType.Timing, Value = 14.28f },
                new FakeSensor { Name = "tCKAVGmin (Minimum Cycle Time)",   SensorType = SensorType.Timing, Value = 0.357f },
            },
        };
        var dimm1 = new FakeHardware
        {
            Name = "Module B",
            HardwareTypeValue = HardwareType.Memory,
            IdentifierValue = "/memory/dimm/1",
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "tAA (CAS Latency Time)",                SensorType = SensorType.Timing, Value = 14.28f },
                new FakeSensor { Name = "tRCD (RAS to CAS Delay Time)",          SensorType = SensorType.Timing, Value = 14.28f },
                new FakeSensor { Name = "tRP (Row Precharge Delay Time)",        SensorType = SensorType.Timing, Value = 14.28f },
                new FakeSensor { Name = "tRAS (Active to Precharge Delay Time)", SensorType = SensorType.Timing, Value = 28.56f },
                new FakeSensor { Name = "tCKAVGmin (Minimum Cycle Time)",        SensorType = SensorType.Timing, Value = float.NaN },
            },
        };

        var modules = SnapshotBuilder.CollectRamModules(new IHardware[] { dimm0, dimm1 });

        Assert.NotNull(modules);
        Assert.Equal(2, modules!.Count);
        Assert.Equal(16.0, modules[0].CapacityGb);
        Assert.Null(modules[0].Timings);
        Assert.Null(modules[1].Timings);
    }

    [Fact]
    public void BuildStorage_reads_wear_thresholds_and_lifetime_totals()
    {
        var disk = new FakeStorage
        {
            Name = "Samsung 990 Pro",
            IdentifierValue = "/nvme/0",
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "Temperature",          SensorType = SensorType.Temperature, Value = 52f },
                new FakeSensor { Name = "Percentage Used",      SensorType = SensorType.Level,       Value = 4f },
                new FakeSensor { Name = "Warning Temperature",  SensorType = SensorType.Temperature, Value = 82f },
                new FakeSensor { Name = "Critical Temperature", SensorType = SensorType.Temperature, Value = 85f },
                new FakeSensor { Name = "Data Read",            SensorType = SensorType.Data,        Value = 21500f },
                new FakeSensor { Name = "Data Written",         SensorType = SensorType.Data,        Value = 18400f },
            },
        };

        var snap = SnapshotBuilder.Build(
            new IHardware[] { disk },
            default,
            new Dictionary<string, long>(),
            new Dictionary<int, PhysicalDiskInfo>());

        var s = snap.Storage![0];
        Assert.Equal(52.0, s.Temp);
        Assert.Equal(4.0, s.PercentageUsedPct);
        Assert.Equal(82.0, s.TempWarnC);
        Assert.Equal(85.0, s.TempCritC);
        Assert.Equal(21500.0, s.DataReadGb);
        Assert.Equal(18400.0, s.DataWrittenGb);
    }

    [Fact]
    public void BuildBoard_carries_motherboard_name()
    {
        var io = new FakeHardware
        {
            Name = "Nuvoton NCT6798D",
            SensorsArray = new ISensor[]
            {
                new FakeSensor { Name = "System", SensorType = SensorType.Temperature, Value = 38f },
            },
        };
        var board = new FakeHardware
        {
            Name = "ASUS TUF GAMING B650-PLUS",
            HardwareTypeValue = HardwareType.Motherboard,
            SubHardwareArray = new IHardware[] { io },
        };

        var info = SnapshotBuilder.BuildBoard(board);

        Assert.NotNull(info);
        Assert.Equal("ASUS TUF GAMING B650-PLUS", info!.Name);
        Assert.Equal(38.0, info.Temp);
    }

    [Theory]
    [InlineData(null, 14.28, 14.28, 28.56, 0.357)]   // tAA missing
    [InlineData(14.28, null, 14.28, 28.56, 0.357)]   // tRCD missing
    [InlineData(14.28, 14.28, 14.28, 28.56, null)]   // tCKAVGmin missing
    [InlineData(14.28, 14.28, 14.28, 28.56, 0.0)]    // zero cycle time
    [InlineData(14.28, 14.28, 14.28, 28.56, -0.357)] // negative cycle time
    public void FormatTimings_returns_null_unless_full_finite_set(
        double? tAa, double? tRcd, double? tRp, double? tRas, double? tCk)
        => Assert.Null(SnapshotBuilder.FormatTimings(tAa, tRcd, tRp, tRas, tCk));

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
        public string IdentifierValue { get; set; } = "fake-hw";
        public Identifier Identifier
        {
            get
            {
                // Identifier(params string[]) joins segments with '/' and prefixes '/'.
                // Splitting our value reproduces the SnapshotBuilder-visible string.
                string[] parts = IdentifierValue.TrimStart('/').Split('/');
                return new Identifier(parts);
            }
        }
        public HardwareType HardwareTypeValue { get; set; } = HardwareType.Cpu;
        public HardwareType HardwareType => HardwareTypeValue;
        public IHardware Parent => null!;
        public IHardware[] SubHardwareArray { get; init; } = Array.Empty<IHardware>();
        public IHardware[] SubHardware => SubHardwareArray;
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
        public FakeStorage()
        {
            HardwareTypeValue = HardwareType.Storage;
            IdentifierValue = "/nvme/0";
        }
    }

    /// <summary>
    /// IHardware whose <see cref="IHardware.Sensors"/> getter always throws.
    /// Used to verify <see cref="SnapshotBuilder.Build"/> isolates per-section
    /// exceptions so the snapshot still emits with that section null.
    /// </summary>
    private sealed class ThrowingFakeHardware : FakeHardware
    {
        public ThrowingFakeHardware(HardwareType type)
        {
            HardwareTypeValue = type;
        }

        public override ISensor[] Sensors
            => throw new InvalidOperationException("sensor read failed");
    }
}
