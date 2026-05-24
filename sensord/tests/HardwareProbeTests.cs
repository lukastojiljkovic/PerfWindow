using System;
using System.IO;
using Sensord.Sensors;
using Xunit;

namespace Sensord.Tests;

public class HardwareProbeTests
{
    [Fact]
    public void DumpHeader_writes_the_expected_label_lines()
    {
        using var sw = new StringWriter();
        HardwareProbe.DumpHeader(sw);
        string output = sw.ToString();
        Assert.Contains("=== sensord hardware probe ===", output);
        Assert.Contains("elevated         :", output);
        Assert.Contains("Path.GetTempPath :", output);
        Assert.Contains("TMP  env         :", output);
        Assert.Contains("TEMP env         :", output);
    }

    [Fact]
    public void DumpHeader_reflects_the_processs_TMP_env()
    {
        string? prev = Environment.GetEnvironmentVariable("TMP");
        Environment.SetEnvironmentVariable("TMP", @"C:\Custom\Tmp");
        try
        {
            using var sw = new StringWriter();
            HardwareProbe.DumpHeader(sw);
            Assert.Contains(@"C:\Custom\Tmp", sw.ToString());
        }
        finally
        {
            Environment.SetEnvironmentVariable("TMP", prev);
        }
    }
}
