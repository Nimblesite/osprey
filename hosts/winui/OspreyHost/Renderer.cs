using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Osprey.Host.WinUI;

/// <summary>The region and revision a node was drawn in, which every control event names.</summary>
readonly record struct Scope(string Region, long Revision);

/// <summary>
/// Draws regions with WinUI controls [WINUI-HOST-RENDER]. Each region is drawn into the holder its
/// <c>slot</c> placed, so a region Osprey did not resend keeps its controls. A kind or symbol this
/// host does not know is drawn as a visible diagnostic and reported, never as a guess.
/// </summary>
sealed partial class Renderer
{
    readonly Dictionary<string, Border> holders = [];
    readonly Dictionary<string, UIElement> drawn = [];
    readonly System.Runtime.CompilerServices.ConditionalWeakTable<ListView, Action<object>> clicks = new();
    readonly Action<string, long, long, object?> control;
    readonly Action<IReadOnlyList<string>> report;
    readonly IReadOnlyDictionary<string, Func<Scope, JsonElement, UIElement>> kinds;
    IReadOnlyList<double> spacing = [4, 8, 12, 20, 28, 36];
    RegionStore regions = new();

    public Renderer(Border root, Action<string, long, long, object?> control, Action<IReadOnlyList<string>> report)
    {
        holders[RegionStore.Root] = root;
        this.control = control;
        this.report = report;
        kinds = Kinds();
        Declared();
    }

    /// <summary>The renderer draws exactly the kinds the host declares drawn [NATIVE-HOSTS-COVERAGE].</summary>
    void Declared()
    {
        var missing = Coverage.Drawn.Except(kinds.Keys).Concat(kinds.Keys.Except(Coverage.Drawn)).ToList();
        if (missing.Count > 0)
        {
            throw new HostException($"The renderer and its coverage declaration disagree on {string.Join(", ", missing)}");
        }
    }

    /// <summary>Takes the spacing scale from the first envelope's theme.</summary>
    public void Themed(JsonElement theme)
    {
        if (theme.ValueKind == JsonValueKind.Object)
        {
            spacing = Envelopes.Items(theme, "spacing", step => step.GetDouble());
        }
    }

    /// <summary>Redraws every region an envelope patched or removed, in the order it listed them.</summary>
    public void Refresh(RegionStore store, IReadOnlyList<string> changed)
    {
        regions = store;
        foreach (var key in changed)
        {
            Redraw(key);
        }
    }

    void Redraw(string key)
    {
        var region = regions.Find(key);
        if (region is null)
        {
            drawn.Remove(key);
            Hold(key, null);
            return;
        }
        drawn[key] = Draw(new Scope(key, region.Revision), region.Node);
        Hold(key, drawn[key]);
    }

    void Hold(string key, UIElement? element)
    {
        if (holders.TryGetValue(key, out var holder))
        {
            holder.Child = null;
            Detach(element);
            holder.Child = element;
        }
    }

    static void Detach(UIElement? element)
    {
        if (element is FrameworkElement { Parent: Border previous })
        {
            previous.Child = null;
        }
    }

    /// <summary>A holder for the region named by a <c>slot</c>, filled with what that region last drew.</summary>
    UIElement Slot(Scope _, JsonElement node)
    {
        var key = Envelopes.Text(node, "key");
        var holder = new Border();
        if (holders.TryGetValue(key, out var previous))
        {
            previous.Child = null;
        }
        holders[key] = holder;
        Hold(key, drawn.GetValueOrDefault(key));
        return holder;
    }

    /// <summary>One node, drawn by its kind.</summary>
    UIElement Draw(Scope scope, JsonElement node) =>
        kinds.TryGetValue(Envelopes.Text(node, "kind"), out var draw)
            ? draw(scope, node)
            : Refused($"This host does not draw {Envelopes.Text(node, "kind")} nodes");

    IEnumerable<UIElement> Children(Scope scope, JsonElement node, string member = "children") =>
        Envelopes.Items(node, member, child => Draw(scope, child));

    /// <summary>The value a control delivers, sent under its issued binding.</summary>
    Action<object?> Emit(Scope scope, JsonElement node)
    {
        var ordinal = Envelopes.Number(node, "event");
        return value => control(scope.Region, scope.Revision, ordinal, value);
    }

    /// <summary>A visible diagnostic in place of what could not be drawn, also reported to the info bar.</summary>
    UIElement Refused(string problem)
    {
        report([problem]);
        return new TextBlock { Text = problem, Foreground = Brush("SystemFillColorCriticalBrush"), TextWrapping = TextWrapping.Wrap };
    }

    double Gap(int step) => spacing.Count > step ? spacing[step] : 8;
}
