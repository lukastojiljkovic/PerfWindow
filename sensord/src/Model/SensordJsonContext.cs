using System.Text.Json.Serialization;
// using Sensord.Control;  // uncomment in Task 3

namespace Sensord.Model;

[JsonSourceGenerationOptions(DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull)]
[JsonSerializable(typeof(Snapshot))]
// [JsonSerializable(typeof(ControlMessage))]  // uncomment in Task 3
public partial class SensordJsonContext : JsonSerializerContext;
