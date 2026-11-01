using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Osprey.Host.Scenes;

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
    readonly Dictionary<string, Action<object>> clicks = [];
    readonly Dictionary<string, SceneView> scenes = [];
    readonly Dictionary<string, RowDefinition> rooms = [];
    IReadOnlyList<Streamed> stream = [];
    int streamTaken;
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
    public void Refresh(RegionStore store, IReadOnlyList<string> changed, IReadOnlyList<Streamed> streamed)
    {
        regions = store;
        (stream, streamTaken) = (streamed, 0);
        foreach (var key in changed)
        {
            Redraw(key);
        }
        if (streamTaken != stream.Count)
        {
            report([$"The application streamed {stream.Count} shapes and its scenes took {streamTaken}"]);
        }
    }

    /// <summary>
    /// A scene [WINUI-HOST-SCENE]: its own shapes and the next shapes streamed for it, in the order the
    /// framework wrote them, drawn into the view this scene keeps between frames.
    /// </summary>
    UIElement Scene(Scope scope, JsonElement node)
    {
        var count = (int)Math.Clamp(Envelopes.Number(node, "streamed"), 0, int.MaxValue);
        if (count > stream.Count - streamTaken)
        {
            return Refused($"A scene expected {count} streamed shapes and {stream.Count - streamTaken} arrived");
        }
        var mine = stream.Skip(streamTaken).Take(count).ToList();
        streamTaken += count;
        Scenes.Scene scene;
        try
        {
            scene = SceneDecoder.Decode(node, mine);
        }
        catch (HostException problem)
        {
            return Refused($"Invalid scene: {problem.Message}");
        }
        var key = $"{scope.Region}#{Envelopes.Number(node, "event")}";
        if (!scenes.TryGetValue(key, out var view))
        {
            view = new SceneView(problem => report([problem]));
            scenes[key] = view;
        }
        Detach(view.Element);
        view.Show(scene, Emit(scope, node));
        return view.Element;
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
            Settle(key);
        }
    }

    // A slot in a column takes the room left over only while its region fills. A region can change
    // what it shows without its column being redrawn, so the slot's row is settled at every draw.
    void Settle(string key)
    {
        if (rooms.TryGetValue(key, out var row) && regions.Find(key) is { } region)
        {
            row.Height = Fills(region.Node) ? Rest : GridLength.Auto;
        }
    }

    /// <summary>
    /// The size, in window units, of a sized scene that is the whole application, as a bare canvas
    /// such as a skinned player is; null when the root region is anything else.
    /// </summary>
    public (double Width, double Height)? CanvasSize() =>
        regions.Find(RegionStore.Root) is { } root && Envelopes.Text(root.Node, "kind") == "scene"
            ? SceneDecoder.Decode(root.Node, []).Opening
            : null;

    /// <summary>Acts on the first scene drawn, as its keyboard or pointer would; false when none is.</summary>
    public bool InScene(Action<SceneView> act)
    {
        if (scenes.Values.FirstOrDefault() is not { } scene)
        {
            return false;
        }
        act(scene);
        return true;
    }

    // Takes an element kept between draws out of the tree it was last drawn into.
    static void Detach(UIElement? element)
    {
        switch ((element as FrameworkElement)?.Parent)
        {
            case Border border:
                border.Child = null;
                break;
            case Panel panel:
                panel.Children.Remove(element);
                break;
            case ContentControl content:
                content.Content = null;
                break;
            case Viewbox box:
                box.Child = null;
                break;
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
            : Refused(Coverage.Refusal(Envelopes.Text(node, "kind")));

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
