using System.Text.Json;
using System.Text.Json.Serialization;

namespace Sensord.Control;

public record ControlMessage(
    [property: JsonPropertyName("interval_ms")] int? IntervalMs,
    [property: JsonPropertyName("shutdown")] bool Shutdown = false);

public static class ControlReader
{
    public static ControlMessage? Parse(string line)
    {
        if (string.IsNullOrWhiteSpace(line)) return null;
        try
        {
            return JsonSerializer.Deserialize(line, Model.SensordJsonContext.Default.ControlMessage);
        }
        catch (JsonException)
        {
            return null;
        }
    }
}
