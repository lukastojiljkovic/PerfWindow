using System.Runtime.InteropServices;

namespace Sensord.Sensors;

public readonly record struct PagefileInfo(double UsedMb, double TotalMb);

public static class PagefileReader
{
    public static PagefileInfo Read()
    {
        var pi = new PerformanceInformation { cb = (uint)Marshal.SizeOf<PerformanceInformation>() };
        if (!GetPerformanceInfo(ref pi, pi.cb))
            return new PagefileInfo(0, 0);

        double pageBytes = (double)pi.PageSize;
        // Commit limit beyond physical RAM is pagefile-backed.
        double totalMb = Math.Max(0, ((double)pi.CommitLimit - pi.PhysicalTotal) * pageBytes) / (1024 * 1024);
        // Commit charge beyond resident physical memory ~= what is paged out.
        double residentPages = (double)pi.PhysicalTotal - pi.PhysicalAvailable;
        double usedMb = Math.Max(0, ((double)pi.CommitTotal - residentPages) * pageBytes) / (1024 * 1024);
        return new PagefileInfo(Math.Min(usedMb, totalMb), totalMb);
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct PerformanceInformation
    {
        public uint cb;
        public nint CommitTotal, CommitLimit, CommitPeak;
        public nint PhysicalTotal, PhysicalAvailable, SystemCache;
        public nint KernelTotal, KernelPaged, KernelNonpaged;
        public nint PageSize;
        public uint HandleCount, ProcessCount, ThreadCount;
    }

    [DllImport("psapi.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetPerformanceInfo(ref PerformanceInformation pi, uint size);
}
