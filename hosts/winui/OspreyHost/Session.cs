using System.Text.Json;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Osprey.Host.WinUI;

/// <summary>
/// One running application [UI-PROTOCOL]: the opaque state Osprey returns, the region store, the
/// host-held slices and the capabilities in flight. Every call into Osprey happens here, on the UI
/// thread [WINUI-HOST-THREAD]; adapters run off it and their completions come back through the queue.
/// </summary>
sealed class Session
{
    readonly Library library;
    readonly RegionStore regions = new();
    readonly Renderer renderer;
    readonly Capabilities capabilities;
    readonly DispatcherQueue queue;
    readonly Action<IReadOnlyList<string>> report;
    const double FirstFrame = 1.0 / 60;
    const double MaxFrame = 0.1;

    readonly System.Diagnostics.Stopwatch clock = new();
    string state = "";
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
        renderer = new Renderer(root, Control, report);
        capabilities = new Capabilities(pick, new Store(store));
    }

    /// <summary>Starts the application with the launch context, then tells it the window is active.</summary>
    public void Begin(IEnumerable<string> arguments)
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var offset = (long)TimeZoneInfo.Local.GetUtcOffset(DateTime.UtcNow).TotalSeconds;
        Guarded(() => Apply(library.Start(Events.Context(Events.Now(), offset, home, arguments))));
        Send(Events.Lifecycle(Events.Now(), "active"));
    }

    /// <summary>Grants the steady clock <paramref name="frames"/> more frames; false when the clock is not running.</summary>
    public bool Grant(long frames)
    {
        owed += animating ? frames : 0;
        return animating;
    }

    /// <summary>Chooses a row as a click on it would.</summary>
    public bool Choose(ListViewItem row) => renderer.Choose(row);

    /// <summary>The window-unit size of the application when it is one sized scene, else null.</summary>
    public (double Width, double Height)? CanvasSize() => renderer.CanvasSize();

    /// <summary>Presses and releases a key in the scene on screen.</summary>
    public bool Press(string key) => renderer.InScene(scene => scene.Press(key));

    /// <summary>Clicks the scene on screen at a point in its units.</summary>
    public bool Tap(double x, double y) => renderer.InScene(scene => scene.Tap(x, y));

    /// <summary>A control's value under the binding the framework issued.</summary>
    void Control(string region, long revision, long ordinal, object? value) =>
        Send(Events.Control(Events.Now(), region, revision, ordinal, value));

    /// <summary>Sends one event; one raised while an envelope is being applied waits its turn.</summary>
    public void Send(string eventText)
    {
        if (applying)
        {
            queue.TryEnqueue(() => Send(eventText));
            return;
        }
        Guarded(() => Apply(library.Dispatch(state, eventText)));
    }

    void Guarded(Action call)
    {
        try
        {
            call();
        }
        catch (Exception problem) when (problem is HostException or JsonException)
        {
            report([problem.Message]);
        }
    }

    void Apply(string text)
    {
        applying = true;
        try
        {
            var envelope = Envelopes.Parse(text);
            state = envelope.State;
            Accept(envelope);
        }
        finally
        {
            applying = false;
        }
    }

    void Accept(Envelope envelope)
    {
        foreach (var write in envelope.Slices)
        {
            Slices.Put(write.Key, write.Text);
        }
        renderer.Themed(envelope.Theme);
        renderer.Refresh(regions, regions.Apply(envelope), [.. Shapes.Taken]);
        report(envelope.Diagnostics);
        foreach (var request in envelope.Requests)
        {
            Run(request);
        }
    }

    void Run(JsonElement request)
    {
        var ticket = Envelopes.Text(request, "ticket");
        if (Envelopes.Text(request, "kind") == "animate")
        {
            Animate(Envelopes.Flag(request, "on"));
            queue.TryEnqueue(() => Complete(Completion.Done(ticket, "null")));
            return;
        }
        capabilities.Answer(request).ContinueWith(done => queue.TryEnqueue(() => Complete(done.IsCompletedSuccessfully
            ? done.Result
            : Completion.Failed(ticket, done.Exception?.GetBaseException().Message ?? "The request did not finish"))), TaskScheduler.Default);
    }

    /// <summary>
    /// The frame clock [WINUI-HOST-FRAME]: while it runs, every display refresh sends a frame with
    /// the seconds since the previous one, 1/60 for the first and at most 0.1, as the GPUI host does.
    /// A refresh that arrives while an envelope is still being applied is skipped.
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
            Send(Events.Frame(Events.Now(), FirstFrame));
            return;
        }
        var now = clock.Elapsed;
        var elapsed = lastFrame is { } last ? Math.Min(MaxFrame, (now - last).TotalSeconds) : FirstFrame;
        lastFrame = now;
        Send(Events.Frame(Events.Now(), elapsed));
    }

    void Complete(Completion completion)
    {
        foreach (var write in completion.Writes)
        {
            Slices.Put(write.Key, write.Text);
        }
        Send(Events.Completed(Events.Now(), completion));
    }
}
