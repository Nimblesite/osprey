namespace Osprey.Host;

/// <summary>
/// The frame viewport fields [UI-PROTOCOL]: this host draws neither a frame sized to the window nor
/// a scrolling frame, so it refuses a told frame that asks for one instead of drawing it as if the
/// field were unset. The check is main's, over told nodes rather than decoded JSON.
/// </summary>
public static class Viewports
{
    /// <summary>The only inset this host draws: a frame sized to its content.</summary>
    public const long ContentSized = -1;

    /// <summary>Refuses a region whose frames size themselves to the window or scroll, wherever they nest.</summary>
    public static void Refuse(string key, Node node)
    {
        var pending = new Stack<Node>();
        pending.Push(node);
        while (pending.TryPop(out var item))
        {
            if (item.Kind == NodeKind.Record)
            {
                if (item.Text("kind") == "framed")
                {
                    Framed(key, item);
                }
                foreach (var member in item.Record.Values)
                {
                    pending.Push(member);
                }
            }
            else if (item.Kind == NodeKind.List)
            {
                foreach (var child in item.List)
                {
                    pending.Push(child);
                }
            }
        }
    }

    static void Framed(string key, Node node) => Style(key, node.Member("style"));

    static void Style(string key, Node style)
    {
        if (style.Kind == NodeKind.Nothing)
        {
            return;
        }
        if (!style.IsRecord)
        {
            throw Needs(key, "an object style", style);
        }
        Inset(key, style.Member("viewportHeightInset"));
        Scrolled(key, style.Member("scrollY"));
    }

    static void Inset(string key, Node inset)
    {
        if (inset.Kind == NodeKind.Nothing)
        {
            return;
        }
        if (inset.Kind != NodeKind.Number || inset.Decimal != Math.Truncate(inset.Decimal))
        {
            throw Needs(key, "an integer viewportHeightInset", inset);
        }
        if (inset.Whole != ContentSized)
        {
            throw new HostException($"Frame viewportHeightInset {inset.Whole} in region {key} is not the content-sized default {ContentSized}, which this host draws by sizing the frame to its content");
        }
    }

    static void Scrolled(string key, Node scroll)
    {
        if (scroll.Kind == NodeKind.Nothing || (scroll.Kind == NodeKind.Flag && !scroll.On))
        {
            return;
        }
        if (scroll.Kind == NodeKind.Flag)
        {
            throw new HostException($"Frame scrollY in region {key} asks the frame to scroll, which this host does not draw; the default is false");
        }
        throw Needs(key, "a boolean scrollY", scroll);
    }

    static HostException Needs(string key, string wanted, Node found) =>
        new($"Frame in region {key} has {found.Kind} where it needs {wanted}");
}
