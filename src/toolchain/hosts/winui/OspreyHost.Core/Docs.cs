using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;

namespace Osprey.Host;

/// <summary>
/// The result documents a capability adapter writes and the host reads: JSON text with the keys
/// [UI-EFFECTS] documents, escaped only where JSON requires, so text stays UTF-8. Nothing here is an
/// envelope: the application's requests and views travel as rows, never JSON.
/// </summary>
public static class Docs
{
    static readonly JsonWriterOptions Relaxed = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    /// <summary>One JSON array of <paramref name="items"/>, in order.</summary>
    public static string Array(IEnumerable<string> items)
    {
        using var buffer = new MemoryStream();
        using (var writer = new Utf8JsonWriter(buffer, Relaxed))
        {
            writer.WriteStartArray();
            foreach (var item in items)
            {
                writer.WriteStringValue(item);
            }
            writer.WriteEndArray();
        }
        return Encoding.UTF8.GetString(buffer.ToArray());
    }

    /// <summary>One JSON object holding the members <paramref name="members"/> writes.</summary>
    public static string Object(Action<Utf8JsonWriter> members)
    {
        using var buffer = new MemoryStream();
        using (var writer = new Utf8JsonWriter(buffer, Relaxed))
        {
            writer.WriteStartObject();
            members(writer);
            writer.WriteEndObject();
        }
        return Encoding.UTF8.GetString(buffer.ToArray());
    }

    /// <summary>A member, or an undefined element when it is absent.</summary>
    public static JsonElement Member(JsonElement item, string name) =>
        item.ValueKind == JsonValueKind.Object && item.TryGetProperty(name, out var value) ? value : default;

    /// <summary>A string member; absent reads as empty, any other kind is malformed.</summary>
    public static string Text(JsonElement item, string name) => Member(item, name) switch
    {
        { ValueKind: JsonValueKind.String } value => value.GetString() ?? "",
        { ValueKind: JsonValueKind.Undefined or JsonValueKind.Null } => "",
        var other => throw Malformed($"a string {name}", other),
    };

    /// <summary>An integer member; absent reads as 0, any other kind is malformed.</summary>
    public static long Number(JsonElement item, string name) => Member(item, name) switch
    {
        { ValueKind: JsonValueKind.Number } value when value.TryGetInt64(out var whole) => whole,
        { ValueKind: JsonValueKind.Undefined or JsonValueKind.Null } => 0,
        var other => throw Malformed($"an integer {name}", other),
    };

    /// <summary>A boolean member; absent reads as false.</summary>
    public static bool Flag(JsonElement item, string name) => Member(item, name).ValueKind == JsonValueKind.True;

    /// <summary>An array member mapped item by item; absent reads as empty.</summary>
    public static IReadOnlyList<T> Items<T>(JsonElement item, string name, Func<JsonElement, T> map) => Member(item, name) switch
    {
        { ValueKind: JsonValueKind.Array } array => [.. array.EnumerateArray().Select(map)],
        { ValueKind: JsonValueKind.Undefined or JsonValueKind.Null } => [],
        var other => throw Malformed($"an array {name}", other),
    };

    static HostException Malformed(string wanted, JsonElement found) =>
        new($"A result document has {found.ValueKind} where it needs {wanted}");
}
