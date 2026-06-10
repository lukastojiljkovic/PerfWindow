using System.Collections.Concurrent;
using System.Net.NetworkInformation;
using System.Runtime.InteropServices;
using System.Text;
using Sensord.Model;

namespace Sensord.Sensors;

/// <summary>
/// Wireless-link details for the active adapter via the Native Wifi API
/// (<c>wlanapi.dll</c>). Every failure path — no WLAN service, no wireless
/// adapter, not connected — returns <c>null</c>; this reader never throws
/// into the snapshot loop.
/// </summary>
public static class WifiReader
{
    private const int WlanIntfOpcodeCurrentConnection = 7;
    private const int WlanInterfaceStateConnected = 1;

    /// <summary>
    /// Set when WlanOpenHandle fails (typically: WLAN AutoConfig service not
    /// running, e.g. desktops without wireless hardware). Once set, every
    /// later call returns immediately — re-probing a dead service each tick
    /// would only burn cycles.
    /// </summary>
    private static volatile bool s_wlanUnavailable;

    /// <summary>
    /// Adapter-name → interface GUID cache (null = not a Wireless80211
    /// adapter). NetworkInterface.GetAllNetworkInterfaces() is too expensive
    /// to repeat per tick for a name that never changes meaning.
    /// </summary>
    private static readonly ConcurrentDictionary<string, Guid?> s_adapterGuids =
        new(StringComparer.OrdinalIgnoreCase);

    /// <summary>
    /// Current connection info for <paramref name="adapterName"/> (the LHM /
    /// NetworkInterface friendly name), or <c>null</c> when the adapter is
    /// not wireless or no connection data is available.
    /// </summary>
    public static WifiInfo? Read(string adapterName)
    {
        if (s_wlanUnavailable) return null;
        try
        {
            Guid? guid = s_adapterGuids.GetOrAdd(adapterName, LookupWirelessGuid);
            if (guid is null) return null;
            return QueryCurrentConnection(guid.Value);
        }
        catch
        {
            return null;
        }
    }

    private static Guid? LookupWirelessGuid(string adapterName)
    {
        try
        {
            foreach (var ni in NetworkInterface.GetAllNetworkInterfaces())
            {
                if (!ni.Name.Equals(adapterName, StringComparison.OrdinalIgnoreCase)) continue;
                if (ni.NetworkInterfaceType != NetworkInterfaceType.Wireless80211) return null;
                return Guid.TryParse(ni.Id, out Guid g) ? g : null;
            }
        }
        catch
        {
            // Enumeration failure — treat as non-wireless; cached like any miss.
        }
        return null;
    }

    private static WifiInfo? QueryCurrentConnection(Guid adapterGuid)
    {
        if (WlanOpenHandle(2, IntPtr.Zero, out _, out IntPtr handle) != 0)
        {
            s_wlanUnavailable = true;
            return null;
        }
        try
        {
            if (WlanEnumInterfaces(handle, IntPtr.Zero, out IntPtr listPtr) != 0)
                return null;
            Guid? target;
            try
            {
                target = PickInterface(listPtr, adapterGuid);
            }
            finally
            {
                WlanFreeMemory(listPtr);
            }
            if (target is null) return null;

            Guid guid = target.Value;
            if (WlanQueryInterface(handle, ref guid, WlanIntfOpcodeCurrentConnection,
                    IntPtr.Zero, out _, out IntPtr dataPtr, IntPtr.Zero) != 0)
                return null;
            try
            {
                var conn = Marshal.PtrToStructure<WlanConnectionAttributes>(dataPtr);
                if (conn.State != WlanInterfaceStateConnected) return null;
                return FromAssociation(
                    conn.Association.Ssid.Ssid,
                    conn.Association.Ssid.Length,
                    conn.Association.SignalQuality,
                    conn.Association.RxRateKbps);
            }
            finally
            {
                WlanFreeMemory(dataPtr);
            }
        }
        finally
        {
            WlanCloseHandle(handle, IntPtr.Zero);
        }
    }

    /// <summary>
    /// Prefer the wlan interface whose GUID matches the adapter; fall back to
    /// the first connected one (covers the rare case where the NetworkInterface
    /// GUID and the wlan interface GUID disagree).
    /// </summary>
    private static Guid? PickInterface(IntPtr listPtr, Guid adapterGuid)
    {
        int count = Marshal.ReadInt32(listPtr);
        // WLAN_INTERFACE_INFO_LIST: dwNumberOfItems, dwIndex, then the array.
        IntPtr items = listPtr + 8;
        int itemSize = Marshal.SizeOf<WlanInterfaceInfo>();
        Guid? firstConnected = null;
        for (int i = 0; i < count; i++)
        {
            var info = Marshal.PtrToStructure<WlanInterfaceInfo>(items + i * itemSize);
            if (info.InterfaceGuid == adapterGuid) return info.InterfaceGuid;
            if (firstConnected is null && info.State == WlanInterfaceStateConnected)
                firstConnected = info.InterfaceGuid;
        }
        return firstConnected;
    }

    /// <summary>
    /// Pure mapping from the decoded WLAN_ASSOCIATION_ATTRIBUTES fields to the
    /// snapshot record. Band stays null: neither the PHY type nor the rate
    /// identifies the radio band reliably, and a wrong band label is worse
    /// than none. Returns null when nothing usable was decoded.
    /// </summary>
    internal static WifiInfo? FromAssociation(byte[]? ssidBytes, uint ssidLength,
                                              uint signalQuality, uint rxRateKbps)
    {
        string? ssid = DecodeSsid(ssidBytes, ssidLength);
        double? signal = signalQuality <= 100 ? signalQuality : null;
        double? phyMbps = rxRateKbps > 0 ? rxRateKbps / 1000.0 : null;
        if (ssid is null && signal is null && phyMbps is null) return null;
        return new WifiInfo(ssid, signal, phyMbps, Band: null);
    }

    /// <summary>
    /// DOT11_SSID decode: <paramref name="length"/> bytes of UTF-8 (the de
    /// facto SSID encoding on modern OSes). Null for empty/over-length input.
    /// </summary>
    internal static string? DecodeSsid(byte[]? bytes, uint length)
    {
        if (bytes is null || length == 0 || length > (uint)bytes.Length) return null;
        try
        {
            string ssid = Encoding.UTF8.GetString(bytes, 0, (int)length);
            return string.IsNullOrWhiteSpace(ssid) ? null : ssid;
        }
        catch
        {
            return null;
        }
    }

    // ---- wlanapi P/Invoke (thin, untested) ------------------------------

    [DllImport("wlanapi.dll")]
    private static extern uint WlanOpenHandle(uint clientVersion, IntPtr reserved,
        out uint negotiatedVersion, out IntPtr clientHandle);

    [DllImport("wlanapi.dll")]
    private static extern uint WlanCloseHandle(IntPtr clientHandle, IntPtr reserved);

    [DllImport("wlanapi.dll")]
    private static extern uint WlanEnumInterfaces(IntPtr clientHandle, IntPtr reserved,
        out IntPtr interfaceList);

    [DllImport("wlanapi.dll")]
    private static extern uint WlanQueryInterface(IntPtr clientHandle, ref Guid interfaceGuid,
        int opCode, IntPtr reserved, out uint dataSize, out IntPtr data, IntPtr opcodeValueType);

    [DllImport("wlanapi.dll")]
    private static extern void WlanFreeMemory(IntPtr memory);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct WlanInterfaceInfo
    {
        public Guid InterfaceGuid;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string Description;
        public int State;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Dot11Ssid
    {
        public uint Length;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 32)] public byte[] Ssid;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct WlanAssociationAttributes
    {
        public Dot11Ssid Ssid;
        public int BssType;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 6)] public byte[] Bssid;
        public int PhyType;
        public uint PhyIndex;
        public uint SignalQuality;
        public uint RxRateKbps;
        public uint TxRateKbps;
    }

    /// <summary>
    /// WLAN_CONNECTION_ATTRIBUTES prefix. The trailing
    /// WLAN_SECURITY_ATTRIBUTES is deliberately omitted — the native buffer
    /// is larger than this struct, which is safe for PtrToStructure, and we
    /// never read security data.
    /// </summary>
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct WlanConnectionAttributes
    {
        public int State;
        public int ConnectionMode;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string ProfileName;
        public WlanAssociationAttributes Association;
    }
}
