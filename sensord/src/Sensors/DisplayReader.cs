using System.Runtime.InteropServices;
using Sensord.Model;

namespace Sensord.Sensors;

/// <summary>
/// Enumerates every attached monitor via <c>EnumDisplayMonitors</c> and reads
/// each one's current mode through <c>EnumDisplaySettings</c>. The first
/// returned entry is the primary display (matches <c>MONITORINFOEX.dwFlags
/// MONITORINFOF_PRIMARY</c>).
///
/// Requires the host process to be Per-Monitor-Aware; otherwise Win32 returns
/// virtualised modes (e.g. 1024x768@60Hz for a 1920x1080@75Hz panel) and the
/// dashboard footer renders those bogus values. Sensord opts in at startup via
/// <c>SetProcessDpiAwarenessContext</c> in <c>Program.MakeProcessDpiAware</c>.
/// </summary>
public static class DisplayReader
{
    private const int ENUM_CURRENT_SETTINGS = -1;
    private const uint MONITORINFOF_PRIMARY = 0x00000001;

    public static IReadOnlyList<DisplayInfo> ReadAll()
    {
        var results = new List<DisplayInfo>();
        try
        {
            EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero,
                (IntPtr hMon, IntPtr _, ref RECT _, IntPtr _) =>
                {
                    var mi = new MONITORINFOEX { cbSize = Marshal.SizeOf<MONITORINFOEX>() };
                    if (!GetMonitorInfo(hMon, ref mi)) return true;

                    var dev = new DEVMODE { dmSize = (ushort)Marshal.SizeOf<DEVMODE>() };
                    if (!EnumDisplaySettings(mi.szDevice, ENUM_CURRENT_SETTINGS, ref dev))
                        return true;

                    var info = new DisplayInfo(
                        Name: mi.szDevice,
                        Width: dev.dmPelsWidth,
                        Height: dev.dmPelsHeight,
                        RefreshHz: dev.dmDisplayFrequency);

                    if ((mi.dwFlags & MONITORINFOF_PRIMARY) != 0)
                        results.Insert(0, info);
                    else
                        results.Add(info);
                    return true;
                }, IntPtr.Zero);
        }
        catch
        {
            // Fall through; return whatever we collected before the failure.
        }
        return results;
    }

    /// <summary>Primary display only. Preserved for backward-compat with v0.8.x dashboards.</summary>
    public static DisplayInfo? Read()
    {
        var all = ReadAll();
        return all.Count > 0 ? all[0] : null;
    }

    private delegate bool MonitorEnumProc(IntPtr hMonitor, IntPtr hdc, ref RECT rect, IntPtr data);

    [DllImport("user32.dll")]
    private static extern bool EnumDisplayMonitors(IntPtr hdc, IntPtr lprcClip, MonitorEnumProc lpfn, IntPtr dwData);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "GetMonitorInfoW")]
    private static extern bool GetMonitorInfo(IntPtr hMonitor, ref MONITORINFOEX lpmi);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern bool EnumDisplaySettings(string? deviceName, int modeNum, ref DEVMODE devMode);

    [StructLayout(LayoutKind.Sequential)]
    private struct RECT { public int Left, Top, Right, Bottom; }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct MONITORINFOEX
    {
        public int cbSize;
        public RECT rcMonitor;
        public RECT rcWork;
        public uint dwFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string szDevice;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct DEVMODE
    {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmDeviceName;
        public ushort dmSpecVersion;
        public ushort dmDriverVersion;
        public ushort dmSize;
        public ushort dmDriverExtra;
        public uint dmFields;
        public int dmPositionX;
        public int dmPositionY;
        public uint dmDisplayOrientation;
        public uint dmDisplayFixedOutput;
        public short dmColor;
        public short dmDuplex;
        public short dmYResolution;
        public short dmTTOption;
        public short dmCollate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmFormName;
        public ushort dmLogPixels;
        public uint dmBitsPerPel;
        public int dmPelsWidth;
        public int dmPelsHeight;
        public uint dmDisplayFlags;
        public int dmDisplayFrequency;
        public uint dmICMMethod;
        public uint dmICMIntent;
        public uint dmMediaType;
        public uint dmDitherType;
        public uint dmReserved1;
        public uint dmReserved2;
        public uint dmPanningWidth;
        public uint dmPanningHeight;
    }
}
