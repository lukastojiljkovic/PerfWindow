using System.Runtime.InteropServices;
using Sensord.Model;

namespace Sensord.Sensors;

/// <summary>
/// Reads the primary monitor's current resolution and refresh rate via the
/// Win32 <c>EnumDisplaySettings</c> API. Cheap (no driver, no kernel call),
/// safe to invoke on every snapshot.
///
/// Returns <c>null</c> only when the Win32 call fails, which on Windows 10+
/// only happens when no display is attached (headless / RDP without a virtual
/// display) — in which case there is no display info to surface anyway.
/// </summary>
public static class DisplayReader
{
    public static DisplayInfo? Read()
    {
        try
        {
            var devMode = new DEVMODE
            {
                dmSize = (ushort)Marshal.SizeOf<DEVMODE>(),
            };
            // device_name = null = primary display, ENUM_CURRENT_SETTINGS = -1
            if (!EnumDisplaySettings(null, ENUM_CURRENT_SETTINGS, ref devMode))
                return null;
            return new DisplayInfo(
                Width: devMode.dmPelsWidth,
                Height: devMode.dmPelsHeight,
                RefreshHz: devMode.dmDisplayFrequency);
        }
        catch
        {
            return null;
        }
    }

    private const int ENUM_CURRENT_SETTINGS = -1;

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern bool EnumDisplaySettings(string? deviceName, int modeNum, ref DEVMODE devMode);

    /// <summary>
    /// Subset of the Win32 <c>DEVMODEW</c> struct — only the fields we read.
    /// Padding/order must match the full struct; the unused fields stay as
    /// zero/empty bytes and are skipped by the marshaller.
    /// </summary>
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct DEVMODE
    {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string dmDeviceName;
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
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string dmFormName;
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
