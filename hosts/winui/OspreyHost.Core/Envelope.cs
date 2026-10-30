using System.Text.Json;

namespace Osprey.Host;

/// <summary>A host-held slice the application wrote in this update [UI-STORE].</summary>
public sealed record SliceWrite(string Key, string Text);

/// <summary>A keyed region to replace, at its new revision [UI-REGIONS].</summary>
public sealed record Patch(string Key, long Revision, JsonElement Node);

/// <summary>
/// One envelope [UI-PROTOCOL]: the opaque state to return with the next event, the slices written,
/// the regions to patch and remove, the capability requests, the diagnostics and, first time
/// only, the theme.
/// </summary>
public sealed record Envelope(
    string State,
    IReadOnlyList<SliceWrite> Slices,
    IReadOnlyList<Patch> Patches,
    IReadOnlyList<string> Removed,
    IReadOnlyList<JsonElement> Requests,
    IReadOnlyList<string> Diagnostics,
    JsonElement Theme);

/// <summary>Decodes envelopes within the host's bounds; anything malformed is refused, never guessed.</summary>
public static class Envelopes
{
    /// <summary>The protocol version this host speaks; any other is refused.</summary>
    public const long Version = 1;

    /// <summary>The longest envelope this host decodes, in characters.</summary>
    public const int MaxLength = 64 * 1024 * 1024;

    static readonly JsonDocumentOptions Bounds = new() { MaxDepth = 256 };

    /// <summary>Decodes one envelope the application returned.</summary>
    public static Envelope Parse(string text)
    {
        if (text.Length > MaxLength)
        {
            throw new HostException($"The envelope is {text.Length} characters, over this host's {MaxLength}");
        }
        using var document = JsonDocument.Parse(text, Bounds);
        var root = document.RootElement.Clone();
        Versioned(root);
        return new Envelope(Text(root, "state"), Items(root, "slices", Slice), Items(root, "patches", Patched),
            Items(root, "removed", Word), Items(root, "requests", item => item), Items(root, "diagnostics", Word), Member(root, "theme"));
    }

    static void Versioned(JsonElement root)
    {
        var version = Member(root, "version");
        if (version.ValueKind != JsonValueKind.Number || version.GetInt64() != Version)
        {
            throw new HostException($"The envelope speaks protocol {version}, and this host speaks {Version}");
        }
    }

    static SliceWrite Slice(JsonElement item) => new(Text(item, "key"), Text(item, "text"));

    static Patch Patched(JsonElement item) => new(Text(item, "key"), Number(item, "revision"), Member(item, "node"));

    static string Word(JsonElement item) =>
        item.ValueKind == JsonValueKind.String ? item.GetString() ?? "" : throw Malformed("a string", item);

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
        new($"The envelope has {found.ValueKind} where it needs {wanted}");
}
