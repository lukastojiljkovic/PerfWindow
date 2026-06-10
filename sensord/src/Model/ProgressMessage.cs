using System.Text.Json.Serialization;

namespace Sensord.Model;

/// <summary>
/// Staged-init progress line, emitted between snapshots while sensor
/// categories are still being enabled. Deliberately NOT snapshot-shaped (no
/// <c>v</c>/<c>ts</c>): pre-0.10 dashboards fail to parse it as a snapshot
/// and drop the line, which keeps the protocol backward compatible.
/// </summary>
public sealed record ProgressMessage(
    [property: JsonPropertyName("progress")] ProgressInfo Progress);

/// <summary>
/// <c>loading</c> is the category currently being enabled — <c>null</c> once
/// all are done, written explicitly (overriding the context-wide null
/// omission) so the final line matches the protocol contract verbatim.
/// Category ids use the canonical init order: cpu, ram, motherboard, gpu,
/// storage, network, controller, battery.
/// </summary>
public sealed record ProgressInfo(
    [property: JsonPropertyName("loading"), JsonIgnore(Condition = JsonIgnoreCondition.Never)] string? Loading,
    [property: JsonPropertyName("done")] IReadOnlyList<string> Done,
    [property: JsonPropertyName("pending")] IReadOnlyList<string> Pending);
