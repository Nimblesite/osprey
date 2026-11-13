using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Osprey.Host.WinUI;

/// <summary>The restyle delegate's shape: a row is restyled from the whole view, so a picker can rebuild its items from its children.</summary>
delegate void Restyling(Shown shown, int index);

/// <summary>
/// The shared view [NATIVE-SHM-VIEW], and this host's only path: the rows an application wrote into
/// the shared frame, drawn as real WinUI controls. Every call commits its frame, leases all four of
/// its runs, and reads them through <see cref="FrameReader"/> — the one reader that checks every row
/// exactly as the reference host does [NATIVE-SHM-SAFETY] — then updates the controls it already
/// drew, row by row, so an entry keeps its focus and caret. Every colour, size, weight and space
/// comes from the rows; the host holds no style of its own. A frame that fails any check is refused
/// whole and the last good view stays. A refused event still shows the frame it wrote.
/// </summary>
sealed partial class SharedViewPage
{
    static readonly GridLength Auto = GridLength.Auto;
    static readonly GridLength Star = new(1, GridUnitType.Star);
    const double FirstFrame = 1.0 / 60;
    const double MaxFrame = 0.1;

    readonly Library library;
    readonly Border page;
    readonly DispatcherQueue queue;
    readonly Action<IReadOnlyList<string>> report;
    readonly Capabilities capabilities;
    readonly System.Diagnostics.Stopwatch clock = new();
    readonly List<Seat?> controls = [];
    readonly List<string> seenIcons = [];
    readonly Grid board = new(); // the view's roots, with the sheet overlay over them
    readonly Grid roots = new(); // the rows with no parent, top to bottom
    readonly Grid overlay = new(); // the rows' sheets float here, above the view
    Shown? shown;
    bool applying;
    bool animating;
    TimeSpan? lastFrame;
    long owed;

    public SharedViewPage(Library library, string store, Border page, DispatcherQueue queue, Action<IReadOnlyList<string>> report, Pickers pickers, bool steady)
    {
        this.library = library;
        this.page = page;
        this.queue = queue;
        this.report = report;
        Steady = steady;
        capabilities = new Capabilities(pickers, new Store(store));
        board.Children.Add(roots);
        board.Children.Add(overlay);
        page.Child = board;
    }

    /// <summary>
    /// A steady clock [WINUI-HOST-SELF-TEST] sends only the frames a self-test grants, each exactly
    /// 1/60 s, so a scene stands still between steps and a picture taken after a given number of
    /// frames is the same on every run and every machine.
    /// </summary>
    public bool Steady { get; }

    /// <summary>The frames granted and not yet sent.</summary>
    public long Owed => owed;

    /// <summary>Hands the application its launch arguments in order, then draws the first frame and tells it the window is active.</summary>
    public void Begin(IEnumerable<string> arguments, bool dark)
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var offset = (long)TimeZoneInfo.Local.GetUtcOffset(DateTime.UtcNow).TotalSeconds;
        foreach (var argument in arguments)
        {
            using var call = library.Argument(argument);
            // The argument's frame is committed and checked like every other call's, though the
            // first view is the one `open` draws.
            _ = FrameReader.Read(call.Frame);
            if (call.Status != 0)
            {
                report([$"The application refused argument {argument}: error {call.Status}"]);
            }
        }
        Guarded(() => Settle(library.Open(Now(), offset, Platform, home, dark)));
        Send(Touch.Phase("active"));
    }

    /// <summary>The platform name this host reports [UI-CONTEXT-PLATFORM].</summary>
    public const string Platform = "windows";

    /// <summary>The seconds since the Unix epoch, the clock every event carries.</summary>
    public static long Now() => DateTimeOffset.UtcNow.ToUnixTimeSeconds();

    /// <summary>Sends one typed event; one raised while a frame is being applied waits its turn.</summary>
    public void Send(Touch touch)
    {
        if (applying)
        {
            queue.TryEnqueue(() => Send(touch));
            return;
        }
        Guarded(() => Settle(library.Event(Now(), touch)));
    }

    /// <summary>Sends the control numbered <paramref name="action"/> with its edited words.</summary>
    public void Send(long action, string words) => Send(Touch.Control(action, words));

    /// <summary>The application's appearance changed; its next frame sees the new one.</summary>
    public void Appear(bool dark) => Send(Touch.Appearance(dark));

    /// <summary>Grants the steady clock <paramref name="frames"/> more frames; false when the clock is not running.</summary>
    public bool Grant(long frames)
    {
        owed += animating ? frames : 0;
        return animating;
    }

    /// <summary>The window-unit size of the application when it is one sized scene, else null.</summary>
    public (double Width, double Height)? CanvasSize() =>
        shown is { Roots.Count: 1 } view && view.Controls[view.Roots[0]].Kind == RowKind.Canvas && view.Scenes.TryGetValue(view.Roots[0], out var scene)
            ? scene.Opening
            : null;

    /// <summary>Presses and releases a key in the first scene on screen; false when no scene is drawn.</summary>
    public bool Press(string key)
    {
        var scene = FirstScene();
        if (scene is null)
        {
            return false;
        }
        scene.Press(key);
        return true;
    }

    /// <summary>Drags the scene element announced as <paramref name="label"/> by a distance in scene units; false when no scene is drawn.</summary>
    public bool Drag(string label, double dx, double dy)
    {
        var scene = FirstScene();
        if (scene is null)
        {
            return false;
        }
        scene.Drag(label, dx, dy);
        return true;
    }

    SceneView? FirstScene()
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

    void Guarded(Action call)
    {
        try
        {
            call();
        }
        catch (HostException problem)
        {
            report([problem.Message]);
        }
    }

    // Every call's frame is committed and leased whole; a frame that fails a check keeps the old view.
    void Settle(Sealed call)
    {
        using (call)
        {
            Shown next;
            try
            {
                next = FrameReader.Read(call.Frame);
            }
            catch (HostException problem)
            {
                report([problem.Message]);
                return;
            }
            var diagnostics = next.Diagnostics.ToList();
            if (call.Status != 0 && diagnostics.Count == 0)
            {
                diagnostics.Add($"The application refused its call: error {call.Status}");
            }
            Accept(next, diagnostics);
        }
    }

    void Accept(Shown next, IReadOnlyList<string> diagnostics)
    {
        foreach (var (key, text) in next.Slices)
        {
            Slices.Put(key, text);
        }
        report(diagnostics);
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
        foreach (var ask in next.Requests)
        {
            Ask(ask);
        }
    }

    void Ask(Ask ask)
    {
        if (ask.Kind() == "animate")
        {
            Animate(ask.Flag(1));
            Complete(Completion.Done(ask.Ticket, "null"));
            return;
        }
        Progress.Note($"asked {ask.Ticket}: {ask.Kind()}");
        capabilities.Answer(ask).ContinueWith(done => queue.TryEnqueue(() => Complete(done.IsCompletedSuccessfully
            ? done.Result
            : Completion.Failed(ask.Ticket, done.Exception?.GetBaseException().Message ?? "The request did not finish"))), TaskScheduler.Default);
    }

    void Complete(Completion completion)
    {
        Progress.Note($"answered {completion.Ticket}: {(completion.Ok ? "ok" : completion.Error)}");
        try
        {
            foreach (var write in completion.Writes)
            {
                Slices.Put(write.Key, write.Text);
            }
            Send(Touch.Completed(completion));
        }
        catch (HostException problem)
        {
            report([problem.Message]);
        }
    }

    /// <summary>
    /// The frame clock [WINUI-HOST-FRAME]: while it runs, every display refresh sends a frame with
    /// the seconds since the previous one, 1/60 for the first and at most 0.1, as the GPUI host does.
    /// A refresh that arrives while a frame is still being applied is skipped.
    /// </summary>
    void Animate(bool on)
    {
        if (on == animating)
        {
            return;
        }
        (animating, lastFrame) = (on, null);
        if (on)
        {
            clock.Restart();
            CompositionTarget.Rendering += Tick;
        }
        else
        {
            CompositionTarget.Rendering -= Tick;
        }
    }

    void Tick(object? sender, object args)
    {
        if (applying || (Steady && owed == 0))
        {
            return;
        }
        if (Steady)
        {
            owed--;
            Send(Touch.Frame(FirstFrame));
            return;
        }
        var now = clock.Elapsed;
        var elapsed = lastFrame is { } last ? Math.Min(MaxFrame, (now - last).TotalSeconds) : FirstFrame;
        lastFrame = now;
        Send(Touch.Frame(elapsed));
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
            var control = next.Controls[index];
            if (seat is null || seat.Kind != control.Kind || seat.Parent != control.Parent || seat.Secure != control.Fit.Secure)
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
            var control = next.Controls[index];
            var seat = Seated(next, index);
            seat.Action = control.Action ?? RowsNone;
            seats[index] = seat;
            seat.Parent = control.Parent;
            if (seat.Floating)
            {
                continue; // a sheet already floats in the overlay
            }
            if (seat.Parent == RowsNone)
            {
                Grid.SetRow(seat.Element, roots.RowDefinitions.Count);
                roots.RowDefinitions.Add(new RowDefinition { Height = rooted == 1 || control.Fit.Grow > 0 ? Star : Auto });
                roots.Children.Add(seat.Element);
            }
            else
            {
                Placed(seats[seat.Parent], seat, control);
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
