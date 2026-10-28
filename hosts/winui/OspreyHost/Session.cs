using System.Text.Json;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Controls;

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
    string state = "";
    bool applying;

    public Session(Library library, Border root, DispatcherQueue queue, Action<IReadOnlyList<string>> report, Func<Task<string?>> pick)
    {
        this.library = library;
        this.queue = queue;
        this.report = report;
        renderer = new Renderer(root, Control, report);
        capabilities = new Capabilities(pick);
    }

    /// <summary>Starts the application with the launch context, then tells it the window is active.</summary>
    public void Begin(IEnumerable<string> arguments)
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var offset = (long)TimeZoneInfo.Local.GetUtcOffset(DateTime.UtcNow).TotalSeconds;
        Guarded(() => Apply(library.Start(Events.Context(Events.Now(), offset, home, arguments))));
        Send(Events.Lifecycle(Events.Now(), "active"));
    }

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
        renderer.Refresh(regions, regions.Apply(envelope));
        report(envelope.Diagnostics);
        foreach (var request in envelope.Requests)
        {
            Run(request);
        }
    }

    void Run(JsonElement request)
    {
        var ticket = Envelopes.Text(request, "ticket");
        capabilities.Answer(request).ContinueWith(done => queue.TryEnqueue(() => Complete(done.IsCompletedSuccessfully
            ? done.Result
            : Completion.Failed(ticket, done.Exception?.GetBaseException().Message ?? "The request did not finish"))), TaskScheduler.Default);
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
