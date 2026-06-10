using LibreHardwareMonitor.Hardware;

namespace Sensord.Sensors;

/// <summary>
/// Wraps an LHM <see cref="Computer"/>. In staged mode it opens with only CPU
/// and Memory enabled (first snapshot in ~1-2 s) and the caller flips the
/// remaining categories one at a time via <see cref="EnableNextCategory"/> —
/// LHM adds hardware groups dynamically when an <c>Is*Enabled</c> property is
/// set on an open Computer. Not thread-safe: enabling a category mutates the
/// hardware group list, so it must run on the same thread that traverses
/// <c>Computer.Hardware</c> (the serve loop).
/// </summary>
public sealed class HardwareMonitor : IDisposable
{
    /// <summary>
    /// Staged init order (canonical protocol category ids). CPU and RAM are
    /// not listed because the constructor enables them before Open().
    /// </summary>
    public static readonly IReadOnlyList<string> StagedCategories = new[]
    {
        "motherboard", "gpu", "storage", "network", "controller", "battery",
    };

    private readonly Computer _computer;
    private readonly UpdateVisitor _visitor = new();
    private readonly CategoryStager _stager;

    public HardwareMonitor(bool staged = false)
    {
        _computer = new Computer
        {
            IsCpuEnabled = true,
            IsMemoryEnabled = true,
        };
        if (staged)
        {
            _stager = new CategoryStager(StagedCategories);
        }
        else
        {
            foreach (string category in StagedCategories)
                SetCategoryEnabled(category);
            _stager = CategoryStager.Completed(StagedCategories);
        }
        _computer.Open();
    }

    public IReadOnlyList<string> DoneCategories => _stager.Done;

    public IReadOnlyList<string> PendingCategories => _stager.Pending;

    /// <summary>
    /// Flips the next pending <c>Is*Enabled</c> property on the open Computer.
    /// Returns <c>false</c> when nothing is pending. A throwing enable is
    /// surfaced via <paramref name="error"/> for the caller to log, and the
    /// category is marked done (skipped) so one broken hardware group can
    /// never stall the staged init.
    /// </summary>
    public bool EnableNextCategory(out string enabled, out Exception? error)
    {
        error = null;
        if (!_stager.TryAdvance(out enabled))
            return false;
        try
        {
            SetCategoryEnabled(enabled);
        }
        catch (Exception ex)
        {
            error = ex;
        }
        return true;
    }

    /// <summary>Updates every hardware node and returns the current hardware list.</summary>
    public IReadOnlyList<IHardware> Refresh()
    {
        _computer.Accept(_visitor);
        return _computer.Hardware.ToArray();
    }

    /// <summary>
    /// Current hardware list WITHOUT a sensor update pass — topology reads
    /// (identifier sets) must not pay for a full refresh.
    /// </summary>
    public IReadOnlyList<IHardware> List() => _computer.Hardware.ToArray();

    public void Dispose() => _computer.Close();

    private void SetCategoryEnabled(string category)
    {
        switch (category)
        {
            case "motherboard": _computer.IsMotherboardEnabled = true; break;
            case "gpu": _computer.IsGpuEnabled = true; break;
            case "storage": _computer.IsStorageEnabled = true; break;
            case "network": _computer.IsNetworkEnabled = true; break;
            case "controller": _computer.IsControllerEnabled = true; break;
            case "battery": _computer.IsBatteryEnabled = true; break;
            default:
                throw new ArgumentOutOfRangeException(nameof(category), category, "unknown sensor category");
        }
    }
}

/// <summary>
/// Bookkeeping for the staged category walk, separate from the LHM Computer
/// so the ordering/exhaustion logic is unit-testable without opening real
/// hardware. CPU and RAM are born done — the constructor enables them.
/// </summary>
internal sealed class CategoryStager
{
    private readonly List<string> _done = new() { "cpu", "ram" };
    private readonly List<string> _pending;

    public CategoryStager(IEnumerable<string> pending) => _pending = new List<string>(pending);

    public IReadOnlyList<string> Done => _done;

    public IReadOnlyList<string> Pending => _pending;

    public bool TryAdvance(out string category)
    {
        if (_pending.Count == 0)
        {
            category = string.Empty;
            return false;
        }
        category = _pending[0];
        _pending.RemoveAt(0);
        _done.Add(category);
        return true;
    }

    public static CategoryStager Completed(IEnumerable<string> categories)
    {
        var stager = new CategoryStager(categories);
        while (stager.TryAdvance(out _))
        {
        }
        return stager;
    }
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
