namespace Sensord.Sensors;

public static class NetUtil
{
    /// <summary>Throughput in bytes/s as a percentage (0–100) of the link speed in bits/s.</summary>
    public static double Utilisation(double bytesPerSec, long linkBitsPerSec)
    {
        if (linkBitsPerSec <= 0) return 0.0;
        double pct = bytesPerSec * 8.0 / linkBitsPerSec * 100.0;
        return Math.Clamp(pct, 0.0, 100.0);
    }
}
