using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Osprey.Host.WinUI;

/// <summary>
/// One running application [NATIVE-SHM-EVENT]: the region store, the renderer that draws the
/// component trees the application told [NATIVE-SHM-NODES], the row view that draws a frame of plain
/// view rows, the host-held slices and the capabilities in flight. Every call into Osprey happens
/// here, on the UI thread [WINUI-HOST-THREAD]; adapters run off it and their completions come back
/// through the queue.
/// </summary>
sealed class Session
{
    /// <summary>The platform name this host reports [UI-CONTEXT-PLATFORM].</summary>
    public const string Platform = "windows";

    readonly Library library;
    readonly RegionStore regions = new();
    readonly Renderer renderer;
    readonly SharedViewPage rows;
    readonly Border told = new();
    readonly Capabilities capabilities;
    readonly DispatcherQueue queue;
    readonly Action<IReadOnlyList<string>> report;
    const double FirstFrame = 1.0 / 60;
    const double MaxFrame = 0.1;

    readonly System.Diagnostics.Stopwatch clock = new();
    bool applying;
    bool animating;
    TimeSpan? lastFrame;
    long owed;

    /// <summary>
    /// A steady clock [WINUI-HOST-SELF-TEST] sends only the frames a self-test grants, each exactly
    /// 1/60 s, so the scene stands still between steps and a picture taken after a given number of
    /// frames is the same on every run and every machine.
    /// </summary>
    public bool Steady { get; init; }

    /// <summary>The frames granted and not yet sent.</summary>
    public long Owed => owed;

    public Session(Library library, string store, Border root, DispatcherQueue queue, Action<IReadOnlyList<string>> report, Pickers pick)
    {
        this.library = library;
        this.queue = queue;
        this.report = report;
        rows = new SharedViewPage(Control, Send, report);
        rows.Element.Visibility = Visibility.Collapsed;
        var stage = new Grid();
        stage.Children.Add(told);
        stage.Children.Add(rows.Element);
        root.Child = stage;
        renderer = new Renderer(told, Control, Send, report);
        capabilities = new Capabilities(pick, new Store(store));
    }

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
        Guarded(() => Apply(library.Open(Now(), offset, Platform, home, dark)));
        Send(Touch.Phase("active"));
    }

    /// <summary>The seconds since the Unix epoch, the clock every event carries.</summary>
    public static long Now() => DateTimeOffset.UtcNow.ToUnixTimeSeconds();

    /// <summary>The application's appearance changed; its next frame sees the new one.</summary>
    public void Appear(bool dark) => Send(Touch.Appearance(dark));

    /// <summary>Grants the steady clock <paramref name="frames"/> more frames; false when the clock is not running.</summary>
    public bool Grant(long frames)
    {
        owed += animating ? frames : 0;
        return animating;
    }

    /// <summary>Chooses a row as a click on it would.</summary>
    public bool Choose(ListViewItem row) => renderer.Choose(row);

    /// <summary>The window-unit size of the application when it is one sized scene, else null.</summary>
    public (double Width, double Height)? CanvasSize() => renderer.CanvasSize() ?? rows.CanvasSize();

    /// <summary>Presses and releases a key in the scene on screen; false when no scene is drawn.</summary>
    public bool Press(string key)
    {
        if (OnShow() is not { } scene)
        {
            return false;
        }
        scene.Press(key);
        return true;
    }

    /// <summary>Drags the scene element a screen reader announces as <paramref name="label"/> by a distance in scene units.</summary>
    public bool Drag(string label, double dx, double dy)
    {
        if (OnShow() is not { } scene)
        {
            return false;
        }
        scene.Drag(label, dx, dy);
        return true;
    }

    // The scene the frame on show gave its canvas, from whichever view is drawing.
    SceneView? OnShow() => told.Visibility == Visibility.Visible ? renderer.FirstScene() : rows.FirstScene();

    /// <summary>Sends one typed event; one raised while a frame is being applied waits its turn.</summary>
    public void Send(Touch touch)
    {
        if (applying)
        {
            queue.TryEnqueue(() => Send(touch));
            return;
        }
        Guarded(() => Apply(library.Event(Now(), touch)));
    }

    /// <summary>A control's value under the ordinal the frame issued, as the words an event carries.</summary>
    void Control(long ordinal, object? value)
    {
        string words;
        try
        {
            words = Touch.WordsOf(value);
        }
        catch (HostException problem)
        {
            report([problem.Message]);
            return;
        }
        Send(Touch.Control(ordinal, words));
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
    void Apply(Sealed call)
    {
        using (call)
        {
            Shown next;
            ToldFrame? frame;
            try
            {
                next = FrameReader.Read(call.Frame);
                frame = Told.Frame(next);
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
            Accept(next, frame, diagnostics);
        }
    }

    void Accept(Shown next, ToldFrame? frame, IReadOnlyList<string> diagnostics)
    {
        foreach (var (key, text) in next.Slices)
        {
            Slices.Put(key, text);
        }
        report(diagnostics);
        applying = true;
        try
        {
            if (frame is not null)
            {
                renderer.Themed(frame.Theme);
                renderer.Refresh(regions, regions.Apply(frame), next);
                Show(told, rows.Element);
            }
            else
            {
                // A frame of plain view rows: the row view draws it, unchanged [NATIVE-SHM-VIEW].
                rows.Show(next);
                Show(rows.Element, told);
            }
        }
        finally
        {
            applying = false;
        }
        foreach (var ask in next.Requests)
        {
            Ask(ask);
        }
    }

    // One of the two views draws this frame; the other keeps its controls out of the way.
    static void Show(FrameworkElement view, FrameworkElement hidden)
    {
        view.Visibility = Visibility.Visible;
        hidden.Visibility = Visibility.Collapsed;
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
        foreach (var write in completion.Writes)
        {
            Slices.Put(write.Key, write.Text);
        }
        Send(Touch.Completed(completion));
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
}
