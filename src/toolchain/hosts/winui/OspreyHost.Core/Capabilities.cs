using System.Collections.Concurrent;
using Osprey.Host.Measure;

namespace Osprey.Host;

/// <summary>A host-held slice the application wrote in this update [UI-STORE].</summary>
public sealed record SliceWrite(string Key, string Text);

/// <summary>A capability's outcome under its ticket, and the slices its adapter wrote [UI-EFFECTS].</summary>
public sealed record Completion(string Ticket, bool Ok, string Result, string Error, IReadOnlyList<SliceWrite> Writes)
{
    /// <summary>A success whose result is the raw JSON <paramref name="result"/>.</summary>
    public static Completion Done(string ticket, string result, params SliceWrite[] writes) => new(ticket, true, result, "", writes);

    /// <summary>A failure the application sees as an error under its ticket.</summary>
    public static Completion Failed(string ticket, string error) => new(ticket, false, "null", error, []);
}

/// <summary>The system pickers: a folder, a file of one of the given extensions, or a save destination; null when the user cancels.</summary>
public sealed record Pickers(Func<Task<string?>> Folder, Func<IReadOnlyList<string>, Task<string?>> File, Func<string, Task<string?>> Save);

/// <summary>
/// The capability adapters [WINUI-HOST-CAPABILITIES]: each does what Osprey asked within the bounds
/// Osprey chose and reports what happened. They sort, hide, format and decide nothing. A kind this
/// host does not answer completes with an error naming it, never silently.
/// </summary>
public sealed class Capabilities(Pickers pickers, Store store)
{
    /// <summary>The most entries one measured folder may list.</summary>
    public const long MaxEntries = 100_000;

    /// <summary>The longest walk this host allows, in milliseconds.</summary>
    public const long MaxDeadline = 10 * 60 * 1000;

    /// <summary>Every capability kind this host answers, for its coverage declaration [NATIVE-HOSTS-COVERAGE].</summary>
    public static IReadOnlyList<string> Answered { get; } = ["measure-folders", "reveal", "choose-folder", "choose-file", "save-file", "run", "cancel", "store-read", "store-write", "store-list", "store-delete", "http", "host-call", .. ChildPool.Kinds];

    readonly ConcurrentDictionary<string, CancellationTokenSource> running = new();
    readonly ChildPool children = new();

    /// <summary>The <c>http</c> answer; a self-test puts its double here [WINUI-HOST-SELF-TEST].</summary>
    public Func<string, Ask, CancellationToken, Task<Completion>> Http { get; init; } = Network.Fetch;

    /// <summary>Starts one request; the task completes off the UI thread.</summary>
    public Task<Completion> Answer(Ask ask)
    {
        var ticket = ask.Ticket;
        return ask.Kind() switch
        {
            "measure-folders" => Measure(ticket, ask),
            "reveal" => Task.FromResult(Reveal(ticket, ask)),
            "choose-folder" => Chosen(ticket, pickers.Folder()),
            "choose-file" => Chosen(ticket, pickers.File(ask.Texts(1))),
            "save-file" => Save(ticket, ask),
            "run" => Processes.Run(ticket, ask),
            "cancel" => Task.FromResult(Cancel(ticket, ask)),
            "store-read" or "store-write" or "store-list" or "store-delete" => Task.FromResult(store.Answer(ask)),
            "http" => Tracked(ticket, token => Http(ticket, ask, token)),
            "host-call" when ask.Text(1) == "process" => children.Session(ask),
            "process-start" or "process-write" or "process-read" or "process-stop" => children.Answer(ask),
            var kind => Task.FromResult(Completion.Failed(ticket, $"The Windows host does not answer {kind} requests")),
        };
    }

    Task<Completion> Measure(string ticket, Ask ask)
    {
        var folders = ask.Texts(1);
        var limit = (int)Math.Clamp(ask.Number(2), 0, MaxEntries);
        var deadline = TimeSpan.FromMilliseconds(Math.Clamp(ask.Number(3), 0, MaxDeadline));
        var slice = ask.Text(4);
        return Tracked(ticket, token => Task.Run(() => Walked(ticket, slice, () => FolderWalk.Measure(folders, limit, deadline, token))));
    }

    // A request a `cancel` naming its ticket can stop while it runs.
    Task<Completion> Tracked(string ticket, Func<CancellationToken, Task<Completion>> work)
    {
        var source = new CancellationTokenSource();
        running[ticket] = source;
        return work(source.Token).ContinueWith(done => Finished(ticket, done.IsCompletedSuccessfully
            ? done.Result
            : Completion.Failed(ticket, done.Exception?.GetBaseException().Message ?? "cancelled")), TaskScheduler.Default);
    }

    static Completion Walked(string ticket, string slice, Func<string> walk)
    {
        try
        {
            return Completion.Done(ticket, "null", new SliceWrite(slice, walk()));
        }
        catch (OperationCanceledException)
        {
            return Completion.Failed(ticket, "cancelled");
        }
        catch (Exception problem) when (problem is IOException or UnauthorizedAccessException or HostException)
        {
            return Completion.Failed(ticket, problem.Message);
        }
    }

    Completion Finished(string ticket, Completion completion)
    {
        if (running.TryRemove(ticket, out var source))
        {
            source.Dispose();
        }
        return completion;
    }

    Completion Cancel(string ticket, Ask ask)
    {
        if (running.TryGetValue(ask.Text(1), out var source))
        {
            source.Cancel();
        }
        return Completion.Done(ticket, "null");
    }

    static Completion Reveal(string ticket, Ask ask) =>
        Explorer.Reveal(ask.Text(1)) is { } problem
            ? Completion.Failed(ticket, problem)
            : Completion.Done(ticket, "{}");

    // A chooser completes with the bare path, and with the empty string when it was cancelled.
    static async Task<Completion> Chosen(string ticket, Task<string?> picking) =>
        Completion.Done(ticket, await picking ?? "");

    // A save writes the text the application staged under the request's slice, then answers with the path.
    async Task<Completion> Save(string ticket, Ask ask)
    {
        if (Slices.Get(ask.Text(2)) is not { } staged)
        {
            return Completion.Failed(ticket, "save-file slice is missing");
        }
        var path = await pickers.Save(ask.Text(1));
        if (path is null)
        {
            return Completion.Done(ticket, "");
        }
        try
        {
            await File.WriteAllTextAsync(path, staged);
            return Completion.Done(ticket, path);
        }
        catch (Exception problem) when (problem is IOException or UnauthorizedAccessException or NotSupportedException or ArgumentException)
        {
            return Completion.Failed(ticket, problem.Message);
        }
    }
}
