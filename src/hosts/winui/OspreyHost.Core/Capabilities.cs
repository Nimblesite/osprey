using System.Collections.Concurrent;
using System.Text.Json;
using Osprey.Host.Measure;

namespace Osprey.Host;

/// <summary>A capability's outcome under its ticket, and the slices its adapter wrote [UI-EFFECTS].</summary>
public sealed record Completion(string Ticket, bool Ok, string Result, string Error, IReadOnlyList<SliceWrite> Writes)
{
    /// <summary>A success whose result is the raw JSON <paramref name="result"/>.</summary>
    public static Completion Done(string ticket, string result, params SliceWrite[] writes) => new(ticket, true, result, "", writes);

    /// <summary>A failure the application sees as an error under its ticket.</summary>
    public static Completion Failed(string ticket, string error) => new(ticket, false, "null", error, []);
}

/// <summary>The system pickers: a folder, or a file of one of the given extensions; null when the user cancels.</summary>
public sealed record Pickers(Func<Task<string?>> Folder, Func<IReadOnlyList<string>, Task<string?>> File);

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
    public static IReadOnlyList<string> Answered { get; } = ["measure-folders", "reveal", "choose-folder", "choose-file", "run", "cancel", "store-read", "store-write", "store-list", "store-delete"];

    readonly ConcurrentDictionary<string, CancellationTokenSource> running = new();

    /// <summary>Starts one request; the task completes off the UI thread.</summary>
    public Task<Completion> Answer(JsonElement request)
    {
        var ticket = Envelopes.Text(request, "ticket");
        return Envelopes.Text(request, "kind") switch
        {
            "measure-folders" => Measure(ticket, request),
            "reveal" => Task.FromResult(Reveal(ticket, request)),
            "choose-folder" => Chosen(ticket, pickers.Folder()),
            "choose-file" => Chosen(ticket, pickers.File(Envelopes.Items(request, "types", item => item.GetString() ?? ""))),
            "run" => Processes.Run(ticket, request),
            "cancel" => Task.FromResult(Cancel(ticket, request)),
            "store-read" or "store-write" or "store-list" or "store-delete" => Task.FromResult(store.Answer(ticket, request)),
            var kind => Task.FromResult(Completion.Failed(ticket, $"The Windows host does not answer {kind} requests")),
        };
    }

    Task<Completion> Measure(string ticket, JsonElement request)
    {
        var folders = Envelopes.Items(request, "folders", item => item.GetString() ?? "");
        var limit = (int)Math.Clamp(Envelopes.Number(request, "limit"), 0, MaxEntries);
        var deadline = TimeSpan.FromMilliseconds(Math.Clamp(Envelopes.Number(request, "timeout"), 0, MaxDeadline));
        var slice = Envelopes.Text(request, "slice");
        var source = new CancellationTokenSource();
        running[ticket] = source;
        return Task.Run(() => Walked(ticket, slice, () => FolderWalk.Measure(folders, limit, deadline, source.Token)))
            .ContinueWith(done => Finished(ticket, done.Result), TaskScheduler.Default);
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

    Completion Cancel(string ticket, JsonElement request)
    {
        if (running.TryGetValue(Envelopes.Text(request, "target"), out var source))
        {
            source.Cancel();
        }
        return Completion.Done(ticket, "null");
    }

    static Completion Reveal(string ticket, JsonElement request) =>
        Explorer.Reveal(Envelopes.Text(request, "path")) is { } problem
            ? Completion.Failed(ticket, problem)
            : Completion.Done(ticket, "{}");

    static async Task<Completion> Chosen(string ticket, Task<string?> picking)
    {
        var path = await picking;
        return Completion.Done(ticket, Json.Object(writer =>
        {
            writer.WriteString("path", path ?? "");
            writer.WriteBoolean("cancelled", path is null);
        }));
    }
}
