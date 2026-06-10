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
        var models = ReadModelMap();
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
                        RefreshHz: dev.dmDisplayFrequency,
                        Model: models.TryGetValue(mi.szDevice, out string? model) ? model : null);

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

    /// <summary>
    /// Primary display only. Kept as a separate top-level Snapshot field
    /// alongside the multi-monitor <c>Displays</c> list so older dashboard
    /// builds that only read <c>display</c> keep functioning.
    /// </summary>
    public static DisplayInfo? Read()
    {
        var all = ReadAll();
        return all.Count > 0 ? all[0] : null;
    }

    /// <summary>
    /// Maps each active GDI device name (e.g. <c>\\.\DISPLAY1</c>) to the
    /// monitor's EDID friendly name via QueryDisplayConfig +
    /// DisplayConfigGetDeviceInfo. A failure at any step simply leaves that
    /// display out of the map — the caller renders a null model.
    /// </summary>
    private static Dictionary<string, string> ReadModelMap()
    {
        var map = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        try
        {
            if (GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS,
                    out uint pathCount, out uint modeCount) != 0)
                return map;
            var paths = new DISPLAYCONFIG_PATH_INFO[pathCount];
            var modes = new DISPLAYCONFIG_MODE_INFO[modeCount];
            if (QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, ref pathCount, paths,
                    ref modeCount, modes, IntPtr.Zero) != 0)
                return map;

            for (int i = 0; i < pathCount; i++)
            {
                var source = new DISPLAYCONFIG_SOURCE_DEVICE_NAME
                {
                    header = new DISPLAYCONFIG_DEVICE_INFO_HEADER
                    {
                        type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                        size = (uint)Marshal.SizeOf<DISPLAYCONFIG_SOURCE_DEVICE_NAME>(),
                        adapterId = paths[i].sourceInfo.adapterId,
                        id = paths[i].sourceInfo.id,
                    },
                };
                if (DisplayConfigGetDeviceInfo(ref source) != 0) continue;

                var target = new DISPLAYCONFIG_TARGET_DEVICE_NAME
                {
                    header = new DISPLAYCONFIG_DEVICE_INFO_HEADER
                    {
                        type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                        size = (uint)Marshal.SizeOf<DISPLAYCONFIG_TARGET_DEVICE_NAME>(),
                        adapterId = paths[i].targetInfo.adapterId,
                        id = paths[i].targetInfo.id,
                    },
                };
                if (DisplayConfigGetDeviceInfo(ref target) != 0) continue;

                string? model = CleanModel(target.monitorFriendlyDeviceName);
                if (model is not null && !string.IsNullOrWhiteSpace(source.viewGdiDeviceName))
                    map[source.viewGdiDeviceName] = model;
            }
        }
        catch
        {
            // Model names are cosmetic; whatever was mapped before the failure
            // is still used.
        }
        return map;
    }

    /// <summary>
    /// Normalises an EDID friendly name: trimmed, null when empty/whitespace
    /// (monitors with a blank EDID descriptor report an empty string, which
    /// must render as "no model", not as an empty label).
    /// </summary>
    internal static string? CleanModel(string? friendlyName)
    {
        if (string.IsNullOrWhiteSpace(friendlyName)) return null;
        return friendlyName.Trim();
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

    // ---- QueryDisplayConfig path → EDID friendly name --------------------

    private const uint QDC_ONLY_ACTIVE_PATHS = 2;
    private const uint DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME = 1;
    private const uint DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME = 2;

    [DllImport("user32.dll")]
    private static extern int GetDisplayConfigBufferSizes(uint flags,
        out uint numPathArrayElements, out uint numModeInfoArrayElements);

    [DllImport("user32.dll")]
    private static extern int QueryDisplayConfig(uint flags,
        ref uint numPathArrayElements, [Out] DISPLAYCONFIG_PATH_INFO[] pathArray,
        ref uint numModeInfoArrayElements, [Out] DISPLAYCONFIG_MODE_INFO[] modeInfoArray,
        IntPtr currentTopologyId);

    [DllImport("user32.dll")]
    private static extern int DisplayConfigGetDeviceInfo(ref DISPLAYCONFIG_SOURCE_DEVICE_NAME requestPacket);

    [DllImport("user32.dll")]
    private static extern int DisplayConfigGetDeviceInfo(ref DISPLAYCONFIG_TARGET_DEVICE_NAME requestPacket);

    [StructLayout(LayoutKind.Sequential)]
    private struct LUID
    {
        public uint LowPart;
        public int HighPart;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct DISPLAYCONFIG_PATH_SOURCE_INFO
    {
        public LUID adapterId;
        public uint id;
        public uint modeInfoIdx;
        public uint statusFlags;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct DISPLAYCONFIG_RATIONAL
    {
        public uint Numerator;
        public uint Denominator;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct DISPLAYCONFIG_PATH_TARGET_INFO
    {
        public LUID adapterId;
        public uint id;
        public uint modeInfoIdx;
        public uint outputTechnology;
        public uint rotation;
        public uint scaling;
        public DISPLAYCONFIG_RATIONAL refreshRate;
        public uint scanLineOrdering;
        public int targetAvailable;
        public uint statusFlags;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct DISPLAYCONFIG_PATH_INFO
    {
        public DISPLAYCONFIG_PATH_SOURCE_INFO sourceInfo;
        public DISPLAYCONFIG_PATH_TARGET_INFO targetInfo;
        public uint flags;
    }

    /// <summary>
    /// 64-byte union-carrying struct; the mode payload is never read here, so
    /// the union body is padded out with opaque longs.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    private struct DISPLAYCONFIG_MODE_INFO
    {
        public uint infoType;
        public uint id;
        public LUID adapterId;
        public ulong pad0, pad1, pad2, pad3, pad4, pad5;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct DISPLAYCONFIG_DEVICE_INFO_HEADER
    {
        public uint type;
        public uint size;
        public LUID adapterId;
        public uint id;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct DISPLAYCONFIG_SOURCE_DEVICE_NAME
    {
        public DISPLAYCONFIG_DEVICE_INFO_HEADER header;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string viewGdiDeviceName;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct DISPLAYCONFIG_TARGET_DEVICE_NAME
    {
        public DISPLAYCONFIG_DEVICE_INFO_HEADER header;
        public uint flags;
        public uint outputTechnology;
        public ushort edidManufactureId;
        public ushort edidProductCodeId;
        public uint connectorInstance;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)] public string monitorFriendlyDeviceName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string monitorDevicePath;
    }
}
