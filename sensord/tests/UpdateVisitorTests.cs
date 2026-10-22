using LibreHardwareMonitor.Hardware;
using Microsoft.Extensions.Logging;
using Sensord.Sensors;
using Xunit;

namespace Sensord.Tests;

public class UpdateVisitorTests
{
    [Fact]
    public void A_device_that_throws_does_not_stop_the_others_updating()
    {
        var before = new FakeHardware("/intelcpu/0");
        var superIo = new FakeHardware("/lpc/nct6798d/0");
        var broken = new FakeHardware("/motherboard", throws: true, superIo);
        var after = new FakeHardware("/gpu-nvidia/0");
        var visitor = new UpdateVisitor(new LevelLog());

        // Computer.Traverse is this loop, with no try/catch of its own.
        foreach (var hardware in new[] { before, broken, after })
            hardware.Accept(visitor);

        Assert.Equal(1, before.Updates);
        Assert.Equal(1, superIo.Updates);
        Assert.Equal(1, after.Updates);
    }

    [Fact]
    public void Only_the_first_failure_of_each_device_is_logged_as_a_warning()
    {
        var log = new LevelLog();
        var visitor = new UpdateVisitor(log);
        var nvme = new FakeHardware("/nvme/0", throws: true);
        var hdd = new FakeHardware("/hdd/1", throws: true);

        for (int tick = 0; tick < 3; tick++)
        {
            nvme.Accept(visitor);
            hdd.Accept(visitor);
        }

        Assert.Equal(new[] { LogLevel.Warning, LogLevel.Warning }, log.Levels);
    }

    // ---- Fakes ----------------------------------------------------------

    private sealed class LevelLog : ILogger
    {
        public List<LogLevel> Levels { get; } = new();
        public IDisposable? BeginScope<TState>(TState state) where TState : notnull => null;
        public bool IsEnabled(LogLevel logLevel) => true;
        public void Log<TState>(LogLevel logLevel, EventId eventId, TState state, Exception? exception,
                                Func<TState, Exception?, string> formatter) => Levels.Add(logLevel);
    }

    private sealed class FakeHardware : IHardware
    {
        private readonly bool _throws;

        public FakeHardware(string identifier, bool throws = false, params IHardware[] subHardware)
        {
            Identifier = new Identifier(identifier.TrimStart('/').Split('/'));
            _throws = throws;
            SubHardware = subHardware;
        }

        public int Updates { get; private set; }

        public void Update()
        {
            // The shape of FanControl #3587: LHM's AtaStorage.UpdateSensors threw a
            // NullReferenceException on every tick.
            if (_throws) throw new NullReferenceException();
            Updates++;
        }

        public void Accept(IVisitor visitor) => visitor.VisitHardware(this);
        public Identifier Identifier { get; }
        public IHardware[] SubHardware { get; }
        public ISensor[] Sensors => Array.Empty<ISensor>();
        public string Name { get; set; } = "fake-hw";
        public HardwareType HardwareType => HardwareType.Cpu;
        public IHardware Parent => null!;
        public IDictionary<string, string> Properties => new Dictionary<string, string>();
        public string GetReport() => string.Empty;
        public void Traverse(IVisitor visitor) { }
        public event SensorEventHandler? SensorAdded { add { } remove { } }
        public event SensorEventHandler? SensorRemoved { add { } remove { } }
    }
}
