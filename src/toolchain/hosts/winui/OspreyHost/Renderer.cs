using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Osprey.Host.Scenes;

namespace Osprey.Host.WinUI;

/// <summary>The region a node was drawn in, which a slot and a scene are named by.</summary>
readonly record struct Scope(string Region);

/// <summary>
/// Draws regions with WinUI controls [WINUI-HOST-RENDER], from the component trees the application
/// told [NATIVE-SHM-NODES]. Each region is drawn into the holder its <c>slot</c> placed, so a region
/// Osprey did not resend keeps its controls. A kind or symbol this host does not know is drawn as a
/// visible diagnostic and reported, never as a guess.
/// </summary>
sealed partial class Renderer
{
    readonly Dictionary<string, Border> holders = [];
    readonly Dictionary<string, UIElement> drawn = [];
    readonly Dictionary<string, Action<object>> clicks = [];
    readonly Dictionary<string, SceneView> scenes = [];
    readonly Dictionary<string, Node> sceneNodes = [];
    readonly HashSet<string> shownScenes = [];
    readonly Dictionary<string, RowDefinition> rooms = [];
    readonly Action<long, object?> control;
    readonly Action<Touch> send;
    readonly Action<IReadOnlyList<string>> report;
    readonly IReadOnlyDictionary<string, Func<Scope, Node, UIElement>> kinds;
    IReadOnlyList<double> spacing = [4, 8, 12, 20, 28, 36];
    RegionStore regions = new();
    Shown? shown;

    public Renderer(Border root, Action<long, object?> control, Action<Touch> send, Action<IReadOnlyList<string>> report)
    {
        holders[RegionStore.Root] = root;
        this.control = control;
        this.send = send;
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

    /// <summary>Takes the spacing scale from the told theme.</summary>
    public void Themed(Theme? theme)
    {
        if (theme is not null)
        {
            spacing = theme.Spacing;
        }
    }

    /// <summary>
    /// Redraws every region a told frame changed or dropped, in the order it listed them, then
    /// repaints every other scene from this frame: a scene's shapes live in the frame's scene rows,
    /// not in its region's tree, so an event that changes only the picture leaves the tree equal.
    /// </summary>
    public void Refresh(RegionStore store, IReadOnlyList<string> changed, Shown frame)
    {
        regions = store;
        shown = frame;
        shownScenes.Clear();
        foreach (var key in changed)
        {
            Redraw(key);
        }
        foreach (var (key, view) in scenes)
        {
            if (!shownScenes.Contains(key) && sceneNodes.TryGetValue(key, out var node) && SceneOf(node) is { } scene)
            {
                view.Show(scene, node.Number("event"), send);
            }
        }
    }

    /// <summary>
    /// A scene [WINUI-HOST-SCENE]: the scene rows the frame gave the record at this node's
    /// <c>sceneRow</c>, drawn into the view this scene keeps between frames.
    /// </summary>
    UIElement Scene(Scope scope, Node node)
    {
        if (SceneOf(node) is not { } scene)
        {
            return Refused("A scene the frame gave no scene rows");
        }
        var key = $"{scope.Region}#{node.Number("event")}";
        if (!scenes.TryGetValue(key, out var view))
        {
            view = new SceneView(problem => report([problem]));
            scenes[key] = view;
        }
        sceneNodes[key] = node;
        shownScenes.Add(key);
        Detach(view.Element);
        view.Show(scene, node.Number("event"), send);
        return view.Element;
    }

    // The checked scene the frame gave the told record at `node`, by the row its sceneRow names.
    Scene? SceneOf(Node node) =>
        shown is { } frame && frame.Scenes.TryGetValue((int)Math.Clamp(node.Number("sceneRow"), 0, int.MaxValue), out var scene) ? scene : null;

    void Redraw(string key)
    {
        var region = regions.Find(key);
        if (region is null)
        {
            drawn.Remove(key);
            Hold(key, null);
            return;
        }
        drawn[key] = Draw(new Scope(key), region.Node);
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
        regions.Find(RegionStore.Root) is { } root && root.Node.Text("kind") == "scene"
            ? SceneOf(root.Node)?.Opening
            : null;

    /// <summary>The first scene drawn, or null when none is.</summary>
    public SceneView? FirstScene() => scenes.Values.FirstOrDefault();

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
    UIElement Slot(Scope _, Node node)
    {
        var key = node.Text("key");
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
    UIElement Draw(Scope scope, Node node) =>
        kinds.TryGetValue(node.Text("kind"), out var draw)
            ? draw(scope, node)
            : Refused(Coverage.Refusal(node.Text("kind")));

    IEnumerable<UIElement> Children(Scope scope, Node node, string member = "children") =>
        node.Items(member, child => Draw(scope, child));

    /// <summary>The value a control delivers, sent under its issued ordinal.</summary>
    Action<object?> Emit(Node node)
    {
        var ordinal = node.Number("event");
        return value => control(ordinal, value);
    }

    /// <summary>A visible diagnostic in place of what could not be drawn, also reported to the info bar.</summary>
    UIElement Refused(string problem)
    {
        report([problem]);
        return new TextBlock { Text = problem, Foreground = Brush("SystemFillColorCriticalBrush"), TextWrapping = TextWrapping.Wrap };
    }

    double Gap(int step) => spacing.Count > step ? spacing[step] : 8;
}
