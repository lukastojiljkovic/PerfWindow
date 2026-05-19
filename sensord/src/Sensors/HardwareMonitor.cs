using LibreHardwareMonitor.Hardware;

namespace Sensord.Sensors;

public sealed class HardwareMonitor : IDisposable
{
    private readonly Computer _computer;
    private readonly UpdateVisitor _visitor = new();

    public HardwareMonitor()
    {
        _computer = new Computer
        {
            IsCpuEnabled = true,
            IsGpuEnabled = true,
            IsMemoryEnabled = true,
            IsMotherboardEnabled = true,
            IsStorageEnabled = true,
            IsNetworkEnabled = true,
            IsControllerEnabled = true,
        };
        _computer.Open();
    }

    /// <summary>Updates every hardware node and returns the current hardware list.</summary>
    public IReadOnlyList<IHardware> Refresh()
    {
        _computer.Accept(_visitor);
        return _computer.Hardware.ToArray();
    }

    public void Dispose() => _computer.Close();
}

internal sealed class UpdateVisitor : IVisitor
{
    public void VisitComputer(IComputer computer) => computer.Traverse(this);

    public void VisitHardware(IHardware hardware)
    {
        hardware.Update();
        foreach (IHardware sub in hardware.SubHardware)
            sub.Accept(this);
    }

    public void VisitSensor(ISensor sensor) { }
    public void VisitParameter(IParameter parameter) { }
}
