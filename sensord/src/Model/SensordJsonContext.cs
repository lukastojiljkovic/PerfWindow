using System.Text.Json.Serialization;
using Sensord.Control;

namespace Sensord.Model;

[JsonSourceGenerationOptions(DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull)]
[JsonSerializable(typeof(Snapshot))]
[JsonSerializable(typeof(ControlMessage))]
public partial class SensordJsonContext : JsonSerializerContext;
