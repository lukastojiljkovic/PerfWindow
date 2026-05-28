using Microsoft.Win32;
using System.Security.Principal;
using LibreHardwareMonitor.Hardware;

namespace Sensord.Sensors;

/// <summary>
/// Diagnostic mode: dumps the complete LibreHardwareMonitor hardware/sensor tree
/// to stdout, then exits. Invoked with <c>sensord --probe</c>. Used to see
/// exactly which sensors the library exposes on a given machine — there is no
/// other way to tell a genuine hardware/library limitation apart from a
/// snapshot-mapping bug.
/// </summary>
public static class HardwareProbe
{
    public static void Run(TextWriter w)
    {
        DumpHeader(w);

        try
        {
            using var monitor = new HardwareMonitor();

            // LHM has now extracted and loaded its kernel driver (PawnIO on
            // 0.9.5+, WinRing0 on older builds — we scan both location sets
            // because users may have leftover WinRing0 files from a prior
            // install). Report where the .sys files landed.
            w.WriteLine("--- kernel driver .sys locations (LHM open) ---");
            string localAppData =
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            ScanForDriver(Path.GetTempPath(), w);
            ScanForDriver(Path.Combine(localAppData, "PerfWindow", "driver"), w);
            ScanForDriver(Path.Combine(localAppData, "Temp"), w);
            ScanForDriver(@"C:\Windows\SystemTemp", w);
            w.WriteLine();

            w.WriteLine("--- loaded driver services (ImagePath = where the .sys was written) ---");
            DumpDriverServices(w);
            w.WriteLine();

            // A few sensors (throughput, rate-derived values) only populate
            // after repeated updates; refresh several times before dumping.
            IReadOnlyList<IHardware> hardware = monitor.Refresh();
            for (int i = 0; i < 3; i++)
            {
                Thread.Sleep(400);
                hardware = monitor.Refresh();
            }

            w.WriteLine($"top-level hardware nodes: {hardware.Count}");
            w.WriteLine();
            foreach (IHardware hw in hardware)
                DumpHardware(hw, w, 0);
        }
        catch (Exception ex)
        {
            w.WriteLine($"PROBE ERROR: {ex.GetType().Name}: {ex.Message}");
            w.WriteLine(ex.StackTrace);
        }
    }

    /// <summary>
    /// Writes the diagnostic header lines that precede the LibreHardwareMonitor
    /// section in <see cref="Run"/>: banner, elevation status, temp paths.
    /// Extracted so the header shape can be unit-tested without spinning up
    /// the kernel driver.
    /// </summary>
    public static void DumpHeader(TextWriter w)
    {
        w.WriteLine("=== sensord hardware probe ===");
        w.WriteLine($"elevated         : {IsElevated()}");
        w.WriteLine($"Path.GetTempPath : {Path.GetTempPath()}");
        w.WriteLine($"TMP  env         : {Environment.GetEnvironmentVariable("TMP")}");
        w.WriteLine($"TEMP env         : {Environment.GetEnvironmentVariable("TEMP")}");
        w.WriteLine();
    }

    private static void DumpHardware(IHardware hw, TextWriter w, int depth)
    {
        string indent = new string(' ', depth * 4);
        w.WriteLine($"{indent}[{hw.HardwareType}] \"{hw.Name}\"");
        w.WriteLine($"{indent}  identifier = {hw.Identifier}");

        ISensor[] sensors = hw.Sensors
            .OrderBy(s => s.SensorType.ToString())
            .ThenBy(s => s.Index)
            .ToArray();

        if (sensors.Length == 0)
            w.WriteLine($"{indent}  (no sensors)");
        foreach (ISensor s in sensors)
        {
            string val = s.Value is float f ? f.ToString("0.###") : "null";
            w.WriteLine($"{indent}  - {s.SensorType,-12} #{s.Index,-3} \"{s.Name}\" = {val}");
        }

        foreach (IHardware sub in hw.SubHardware)
            DumpHardware(sub, w, depth + 1);
    }

    private static void ScanForDriver(string dir, TextWriter w)
    {
        try
        {
            if (!Directory.Exists(dir))
            {
                w.WriteLine($"  {dir}  (does not exist)");
                return;
            }
            string[] sys = Directory.GetFiles(dir, "*.sys");
            if (sys.Length == 0)
            {
                w.WriteLine($"  {dir}  (no .sys files)");
                return;
            }
            foreach (string f in sys)
            {
                var fi = new FileInfo(f);
                w.WriteLine($"  {f}  ({fi.Length} B, written {fi.LastWriteTime:HH:mm:ss})");
            }
        }
        catch (Exception ex)
        {
            w.WriteLine($"  {dir}  (scan error: {ex.Message})");
        }
    }

    /// <summary>
    /// Prints every kernel-driver service whose ImagePath looks like a
    /// temp-staged <c>.sys</c> (PawnIO, WinRing0 and friends). LHM keeps its
    /// driver service registered while the monitor is open, so the ImagePath
    /// reveals exactly where the driver file was written even after the file
    /// itself is deleted post-load.
    /// </summary>
    private static void DumpDriverServices(TextWriter w)
    {
        try
        {
            using RegistryKey? services =
                Registry.LocalMachine.OpenSubKey(@"SYSTEM\CurrentControlSet\Services");
            if (services is null)
            {
                w.WriteLine("  (cannot open Services registry key)");
                return;
            }

            bool any = false;
            foreach (string name in services.GetSubKeyNames())
            {
                using RegistryKey? svc = services.OpenSubKey(name);
                if (svc?.GetValue("ImagePath") is not string img || img.Length == 0)
                    continue;
                if (!img.Contains(".sys", StringComparison.OrdinalIgnoreCase))
                    continue;
                if (img.Contains("PerfWindow", StringComparison.OrdinalIgnoreCase)
                    || img.Contains("PawnIO", StringComparison.OrdinalIgnoreCase)
                    || img.Contains("WinRing", StringComparison.OrdinalIgnoreCase)
                    || img.Contains(@"\Temp", StringComparison.OrdinalIgnoreCase)
                    || img.Contains("SystemTemp", StringComparison.OrdinalIgnoreCase))
                {
                    w.WriteLine($"  service '{name}'");
                    w.WriteLine($"    ImagePath = {img}");
                    any = true;
                }
            }
            if (!any)
                w.WriteLine("  (no temp-staged driver services found)");
        }
        catch (Exception ex)
        {
            w.WriteLine($"  (service scan error: {ex.Message})");
        }
    }

    private static bool IsElevated()
    {
        try
        {
            using WindowsIdentity id = WindowsIdentity.GetCurrent();
            return new WindowsPrincipal(id).IsInRole(WindowsBuiltInRole.Administrator);
        }
        catch
        {
            return false;
        }
    }
}
