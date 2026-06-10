using System;
using System.Text;
using Sensord.Sensors;
using Xunit;

namespace Sensord.Tests;

public class WifiReaderTests
{
    private static byte[] SsidBuffer(string ssid, out uint length)
    {
        // Mirrors DOT11_SSID: a fixed 32-byte buffer with a separate length.
        var buf = new byte[32];
        byte[] encoded = Encoding.UTF8.GetBytes(ssid);
        encoded.CopyTo(buf, 0);
        length = (uint)encoded.Length;
        return buf;
    }

    [Fact]
    public void DecodeSsid_decodes_utf8_up_to_length()
    {
        byte[] buf = SsidBuffer("PerfWindow-5G", out uint len);
        Assert.Equal("PerfWindow-5G", WifiReader.DecodeSsid(buf, len));
    }

    [Fact]
    public void DecodeSsid_returns_null_for_missing_empty_or_overlong_input()
    {
        Assert.Null(WifiReader.DecodeSsid(null, 5));
        Assert.Null(WifiReader.DecodeSsid(new byte[32], 0));
        Assert.Null(WifiReader.DecodeSsid(new byte[4], 10));
    }

    [Fact]
    public void FromAssociation_maps_signal_quality_and_rx_rate()
    {
        byte[] buf = SsidBuffer("Lab", out uint len);

        var info = WifiReader.FromAssociation(buf, len, signalQuality: 84, rxRateKbps: 1_201_000);

        Assert.NotNull(info);
        Assert.Equal("Lab", info!.Ssid);
        Assert.Equal(84.0, info.SignalPct);
        Assert.Equal(1201.0, info.PhyMbps);
        Assert.Null(info.Band);
    }

    [Fact]
    public void FromAssociation_returns_null_when_nothing_usable_was_decoded()
    {
        // Quality above 100 is out of the documented range; rate 0 means no link.
        var info = WifiReader.FromAssociation(new byte[32], 0, signalQuality: 101, rxRateKbps: 0);
        Assert.Null(info);
    }

    [Fact]
    public void Read_returns_null_for_an_unknown_adapter_and_never_throws()
    {
        Assert.Null(WifiReader.Read("no-such-adapter-" + Guid.NewGuid().ToString("N")));
    }
}
