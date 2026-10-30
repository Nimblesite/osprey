using System.Text.Json;

namespace Osprey.Host;

/// <summary>
/// The frame viewport fields [UI-PROTOCOL]: this host draws neither a frame sized to the window nor
/// a scrolling frame, so it refuses a frame that asks for one instead of drawing it as if the field
/// were unset.
/// </summary>
public static class Viewports
{
    /// <summary>The only inset this host draws: a frame sized to its content.</summary>
    public const long ContentSized = -1;

    /// <summary>Refuses a patch whose frames size themselves to the window or scroll, wherever they nest.</summary>
    public static void Refuse(string key, JsonElement node)
    {
        var pending = new Stack<JsonElement>();
        pending.Push(node);
        while (pending.TryPop(out var item))
        {
            if (item.ValueKind == JsonValueKind.Object)
            {
                var kind = Envelopes.Member(item, "kind");
                if (kind.ValueKind == JsonValueKind.String && kind.GetString() == "framed") Framed(key, item);
                foreach (var member in item.EnumerateObject()) pending.Push(member.Value);
            }
            else if (item.ValueKind == JsonValueKind.Array)
            {
                foreach (var child in item.EnumerateArray()) pending.Push(child);
            }
        }
    }

    static void Framed(string key, JsonElement node) => Style(key, Envelopes.Member(node, "style"));

    static void Style(string key, JsonElement style)
    {
        if (style.ValueKind == JsonValueKind.Undefined) return;
        if (style.ValueKind != JsonValueKind.Object) throw Needs(key, "an object style", style);
        Inset(key, Envelopes.Member(style, "viewportHeightInset"));
        Scrolled(key, Envelopes.Member(style, "scrollY"));
    }

    static void Inset(string key, JsonElement inset)
    {
        if (inset.ValueKind == JsonValueKind.Undefined) return;
        if (inset.ValueKind != JsonValueKind.Number || !inset.TryGetInt64(out var whole)) throw Needs(key, "an integer viewportHeightInset", inset);
        if (whole != ContentSized)
        {
            throw new HostException($"Frame viewportHeightInset {whole} in region {key} is not the content-sized default {ContentSized}, which this host draws by sizing the frame to its content");
        }
    }

    static void Scrolled(string key, JsonElement scroll)
    {
        if (scroll.ValueKind is JsonValueKind.Undefined or JsonValueKind.False) return;
        if (scroll.ValueKind == JsonValueKind.True)
        {
            throw new HostException($"Frame scrollY in region {key} asks the frame to scroll, which this host does not draw; the default is false");
        }
        throw Needs(key, "a boolean scrollY", scroll);
    }

    static HostException Needs(string key, string wanted, JsonElement found) =>
        new($"Frame in region {key} has {found.ValueKind} where it needs {wanted}");
}
