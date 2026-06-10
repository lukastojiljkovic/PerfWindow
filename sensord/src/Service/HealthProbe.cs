using Sensord.Model;

namespace Sensord.Service;

/// <summary>
/// Classifies the result of a one-shot PawnIO read into a <see cref="HealthInfo"/>
/// that the worker attaches to every snapshot. The caller must pass CPU
/// temperatures only (LHM hardware type Cpu): board or drive thermals come
/// from other chips and would mask a dead MSR driver as healthy (SND-5).
/// <c>ok</c>: at least one CPU temperature reading came back positive.
/// <c>missing</c>: PawnIO appears absent (read succeeded but no CPU thermals).
/// <c>denied</c>: an exception was thrown — typically ACCESS_DENIED on the
/// kernel driver handle.
/// </summary>
internal static class HealthProbe
{
    public static HealthInfo Classify(IEnumerable<double?>? cpuTemps, Exception? exception)
    {
        if (exception is not null)
            return new HealthInfo(
                Pawnio: "denied",
                Degraded: true,
                Notes: $"PawnIO probe threw: {exception.Message}");

        if (cpuTemps is null)
            return new HealthInfo(
                Pawnio: "missing",
                Degraded: true,
                Notes: "PawnIO probe returned no data.");

        bool anyPositive = false;
        foreach (var t in cpuTemps)
            if (t is double v && v > 0)
            {
                anyPositive = true;
                break;
            }

        return anyPositive
            ? new HealthInfo("ok", false, null)
            : new HealthInfo("missing", true, "PawnIO probe returned no CPU temperature.");
    }
}
