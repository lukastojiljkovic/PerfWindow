using LibreHardwareMonitor.Hardware;
using Sensord.Sensors;
using Xunit;

namespace Sensord.Tests;

public class TopologySignatureTests
{
    [Fact]
    public void Compute_EmptyList_ReturnsStableValue()
    {
        Assert.Equal(0, TopologySignature.Compute(Array.Empty<IHardware>()));
    }

    [Fact]
    public void Compute_OrderIndependent()
    {
        IHardware a = new FakeHw("cpu", "0");
        IHardware b = new FakeHw("gpu", "0");
        Assert.Equal(
            TopologySignature.Compute(new[] { a, b }),
            TopologySignature.Compute(new[] { b, a }));
    }

    [Fact]
    public void Compute_DifferentSets_ReturnDifferentValues()
    {
        IHardware a = new FakeHw("cpu", "0");
        IHardware b = new FakeHw("gpu", "0");
        Assert.NotEqual(
            TopologySignature.Compute(new[] { a }),
            TopologySignature.Compute(new[] { a, b }));
    }

    [Fact]
    public void Compute_IncludesSubHardware()
    {
        IHardware sub = new FakeHw("storage", "0", "smart");
        IHardware root = new FakeHw(new[] { "storage", "0" }, new[] { sub });
        Assert.NotEqual(
            TopologySignature.Compute(new[] { root }),
            TopologySignature.Compute(Array.Empty<IHardware>()));
    }

    [Fact]
    public void IdentifierSet_ReturnsEveryIdentifier_FlatAndSub()
    {
        IHardware sub1 = new FakeHw("storage", "0", "smart");
        IHardware sub2 = new FakeHw("storage", "0", "io");
        IHardware root = new FakeHw(new[] { "storage", "0" }, new[] { sub1, sub2 });
        IHardware other = new FakeHw("cpu", "0");

        var set = TopologySignature.IdentifierSet(new[] { root, other });

        Assert.Contains("/storage/0", set);
        Assert.Contains("/storage/0/smart", set);
        Assert.Contains("/storage/0/io", set);
        Assert.Contains("/cpu/0", set);
        Assert.Equal(4, set.Count);
    }

    /// <summary>
    /// Minimal IHardware stub: only Identifier and SubHardware are exercised
    /// by TopologySignature.Compute. Everything else throws or returns
    /// defaults — these tests must not touch them.
    /// </summary>
    private sealed class FakeHw : IHardware
    {
        private readonly Identifier _id;
        private readonly IHardware[] _subs;

        public FakeHw(params string[] idParts)
        {
            _id = new Identifier(idParts);
            _subs = Array.Empty<IHardware>();
        }

        public FakeHw(string[] idParts, IHardware[] subs)
        {
            _id = new Identifier(idParts);
            _subs = subs;
        }

        public Identifier Identifier => _id;
        public IHardware[] SubHardware => _subs;

        public string Name { get => "fake"; set { } }
        public HardwareType HardwareType => HardwareType.Cpu;
        public IHardware Parent => null!;
        public ISensor[] Sensors => Array.Empty<ISensor>();
        public IDictionary<string, string> Properties => new Dictionary<string, string>();
        public string GetReport() => string.Empty;
        public void Update() { }
        public void Accept(IVisitor visitor) { }
        public void Traverse(IVisitor visitor) { }
        public event SensorEventHandler? SensorAdded { add { } remove { } }
        public event SensorEventHandler? SensorRemoved { add { } remove { } }
    }
}
