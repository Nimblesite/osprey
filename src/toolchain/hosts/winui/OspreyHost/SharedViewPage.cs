using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Osprey.Host.WinUI;

/// <summary>The restyle delegate's shape: a row is restyled from the whole view, so a picker can rebuild its items from its children.</summary>
delegate void Restyling(Shown shown, int index);

/// <summary>
/// The shared view [NATIVE-SHM-VIEW]: a frame of plain view rows, drawn as real WinUI controls, row
/// by row, so an entry keeps its focus and caret. A frame that tells components is drawn by the
/// <see cref="Renderer"/> instead [NATIVE-SHM-NODES]; this view is what the Tally example uses. Every
/// colour, size, weight and space comes from the rows; the host holds no style of its own.
/// </summary>
sealed partial class SharedViewPage
{
    static readonly GridLength Auto = GridLength.Auto;
    static readonly GridLength Star = new(1, GridUnitType.Star);

    readonly Action<long, string> control;
    readonly Action<Touch> send;
    readonly Action<IReadOnlyList<string>> report;
    readonly List<Seat?> controls = [];
    readonly List<string> seenIcons = [];
    readonly Grid board = new(); // the view's roots, with the sheet overlay over them
    readonly Grid roots = new(); // the rows with no parent, top to bottom
    readonly Grid overlay = new(); // the rows' sheets float here, above the view
    Shown? shown;
    bool applying;

    public SharedViewPage(Action<long, string> control, Action<Touch> send, Action<IReadOnlyList<string>> report)
    {
        this.control = control;
        this.send = send;
        this.report = report;
        board.Children.Add(roots);
        board.Children.Add(overlay);
    }

    /// <summary>The element that holds the view in the window.</summary>
    public FrameworkElement Element => board;

    /// <summary>Draws one frame of plain view rows, restyling the controls already on screen where it can.</summary>
    public void Show(Shown next)
    {
        applying = true;
        try
        {
            Drawn(next);
        }
        finally
        {
            applying = false;
        }
        shown = next;
    }

    /// <summary>The window-unit size of the application when it is one sized scene, else null.</summary>
    public (double Width, double Height)? CanvasSize() =>
        shown is { Roots.Count: 1 } view && view.Controls[view.Roots[0]].Kind == RowKind.Canvas && view.Scenes.TryGetValue(view.Roots[0], out var scene)
            ? scene.Opening
            : null;

    /// <summary>Presses and releases a key in the first scene on screen; false when no scene is drawn.</summary>
    public bool Press(string key)
    {
        if (FirstScene() is not { } scene)
        {
            return false;
        }
        scene.Press(key);
        return true;
    }

    /// <summary>Drags the scene element announced as <paramref name="label"/> by a distance in scene units; false when no scene is drawn.</summary>
    public bool Drag(string label, double dx, double dy)
    {
        if (FirstScene() is not { } scene)
        {
            return false;
        }
        scene.Drag(label, dx, dy);
        return true;
    }

    /// <summary>The first scene drawn, or null when none is.</summary>
    public SceneView? FirstScene()
    {
        foreach (var seat in controls)
        {
            if (seat?.Scene is { } scene)
            {
                return scene;
            }
        }
        return null;
    }

    void Drawn(Shown next)
    {
        if (controls.Count == next.Controls.Count && Shaped(next))
        {
            Updated(next);
            return;
        }
        Built(next);
    }

    // Whether the next view seats the same controls the same way, so the old ones can be restyled
    // in place. A field that changes to or from a secure one rebuilds the tree.
    bool Shaped(Shown next)
    {
        for (var index = 0; index < next.Controls.Count; index++)
        {
            var seat = controls[index];
            var row = next.Controls[index];
            if (seat is null || seat.Kind != row.Kind || seat.Parent != row.Parent || seat.Secure != row.Fit.Secure)
            {
                return false;
            }
        }
        return true;
    }

    /// <summary>The parent of a root row, and the action of a control that delivers none.</summary>
    const int RowsNone = -1;

    // Restyles the controls already on screen, row by row, so focus and caret never move.
    void Updated(Shown next)
    {
        for (var index = 0; index < next.Controls.Count; index++)
        {
            if (controls[index] is { } seat)
            {
                seat.Action = next.Controls[index].Action ?? RowsNone;
                seat.Restyle?.Invoke(next, index);
            }
        }
    }

    // Builds every control fresh: one seat per row index, the tree the rows name.
    void Built(Shown next)
    {
        var seats = new Seat?[next.Controls.Count];
        var rooted = next.Roots.Count(root => next.Controls[root].Kind != RowKind.Sheet);
        roots.Children.Clear();
        roots.RowDefinitions.Clear();
        overlay.Children.Clear();
        for (var index = 0; index < next.Controls.Count; index++)
        {
            var row = next.Controls[index];
            var seat = Seated(next, index);
            seat.Action = row.Action ?? RowsNone;
            seats[index] = seat;
            seat.Parent = row.Parent;
            if (seat.Floating)
            {
                continue; // a sheet already floats in the overlay
            }
            if (seat.Parent == RowsNone)
            {
                Grid.SetRow(seat.Element, roots.RowDefinitions.Count);
                roots.RowDefinitions.Add(new RowDefinition { Height = rooted == 1 || row.Fit.Grow > 0 ? Star : Auto });
                roots.Children.Add(seat.Element);
            }
            else
            {
                Placed(seats[seat.Parent], seat, row);
            }
        }
        controls.Clear();
        controls.AddRange(seats);
    }

    // Seats one drawn child in its parent: a picker keeps its options as items, a menu shows only
    // its first child, a layer shares its one cell, and the rest take a grid slot sized by the row.
    void Placed(Seat? parent, Seat child, Control row)
    {
        if (parent?.Panel is not { } pile)
        {
            return; // nothing to sit in: the row draws nothing rather than take the host down
        }
        if (parent.Kind == RowKind.Picker || (parent.Kind == RowKind.Menu && parent.Shown++ > 0))
        {
            return; // a picker's options are its items; a menu's other children are its context menu
        }
        if (parent.Kind == RowKind.Layer)
        {
            pile.Children.Add(child.Element); // every layer child shares the one cell; the last is in front
            return;
        }
        Roomed(parent, pile, child, row);
    }

    // One child's slot in its parent's grid: a flexible child takes its share of the room left
    // over, a gap takes one share by definition, and every other child takes what it needs.
    static void Roomed(Seat parent, Grid pile, Seat child, Control row)
    {
        var share = row.Fit.Grow > 0 ? row.Fit.Grow : row.Kind == RowKind.Gap ? 1 : 0;
        var room = share > 0 ? new GridLength(share, GridUnitType.Star) : Auto;
        if (parent.Across)
        {
            Grid.SetColumn(child.Element, pile.ColumnDefinitions.Count);
            pile.ColumnDefinitions.Add(new ColumnDefinition { Width = room });
        }
        else
        {
            Grid.SetRow(child.Element, pile.RowDefinitions.Count);
            pile.RowDefinitions.Add(new RowDefinition { Height = room });
        }
        pile.Children.Add(child.Element);
    }

    /// <summary>One drawn control: what sits in its parent, how a later row restyles it, and the event it delivers.</summary>
    sealed class Seat(FrameworkElement element, RowKind kind)
    {
        public FrameworkElement Element { get; } = element;

        public RowKind Kind { get; } = kind;

        /// <summary>The row it was seated in, kept so the page can see the tree is unchanged.</summary>
        public int Parent { get; set; }

        public Grid? Panel { get; init; }

        public bool Across { get; init; }

        /// <summary>A sheet floats in the page's overlay instead of sitting in its parent.</summary>
        public bool Floating { get; init; }

        /// <summary>A secure field is another control: the view rebuilds when this changes.</summary>
        public bool Secure { get; init; }

        /// <summary>How many of a menu's children were shown in its panel; the rest are its items.</summary>
        public int Shown { get; set; }

        /// <summary>A canvas's scene, kept between frames and given the row's binding.</summary>
        public SceneView? Scene { get; init; }

        public Restyling? Restyle { get; set; }

        public long Action { get; set; }

        /// <summary>Whether an entry delivers its words only when committed with Return.</summary>
        public bool Commit { get; set; }
    }
}
