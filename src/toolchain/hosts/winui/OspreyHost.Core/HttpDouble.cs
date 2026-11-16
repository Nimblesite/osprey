using System.Text.Json;

namespace Osprey.Host;

/// <summary>
/// [WINUI-HOST-SELF-TEST] Answers <c>http</c> from a file of replies, in order, and keeps each request
/// beside the run, as the SwiftUI host's double does: <c>{"models": {...}, "replies": [{"status": 200,
/// "body": "...", "delayMs": 0, "requireUrl": "..."}]}</c>. A provider's model list, a GET to a path
/// ending <c>/models</c>, is answered from <c>models</c> for any provider, or 404 without it. A server
/// on this machine stays real: only the model is the double. The record holds where each request went
/// and what it carried, never the key.
/// </summary>
public sealed class HttpDouble
{
    readonly List<JsonElement> replies;
    readonly string? models;
    readonly string requests;
    int next = -1;

    /// <summary>The double the file at <paramref name="file"/> describes, recording to <paramref name="requests"/>.</summary>
    public HttpDouble(string file, string requests)
    {
        using var document = JsonDocument.Parse(File.ReadAllText(file));
        var root = document.RootElement.Clone();
        if (Docs.Member(root, "replies") is not { ValueKind: JsonValueKind.Array } listed)
        {
            throw new HostException($"The double {file} holds no list of replies");
        }
        replies = [.. listed.EnumerateArray()];
        models = Docs.Member(root, "models") is { ValueKind: JsonValueKind.Object } list ? list.GetRawText() : null;
        this.requests = requests;
    }

    /// <summary>The <c>http</c> answer, in the shape <see cref="Network.Fetch"/> has.</summary>
    public async Task<Completion> Answer(string ticket, Ask ask, CancellationToken cancel)
    {
        var (url, problem) = Network.Allowed(ask.Text(2));
        if (url is null)
        {
            return Completion.Failed(ticket, problem);
        }
        if (Network.IsLoopback(url))
        {
            return await Network.Fetch(ticket, ask, cancel);
        }
        var authorized = ask.Pairs(3).Any(header => header.Name.Equals("Authorization", StringComparison.OrdinalIgnoreCase) && header.Value.StartsWith("Bearer ", StringComparison.Ordinal));
        if (ask.Text(1) == "GET" && url.AbsolutePath.EndsWith("/models", StringComparison.Ordinal) && authorized)
        {
            return models is null ? Status(ticket, 404) : Status(ticket, 200, new SliceWrite(ask.Text(7), models));
        }
        return await Replied(ticket, ask, url, authorized, cancel);
    }

    async Task<Completion> Replied(string ticket, Ask ask, Uri url, bool authorized, CancellationToken cancel)
    {
        var index = Interlocked.Increment(ref next);
        if (index >= replies.Count)
        {
            return Completion.Failed(ticket, $"The double has no reply for request {index + 1}");
        }
        if (ask.Text(1) != "POST" || !authorized)
        {
            return Completion.Failed(ticket, "The double takes only an authorized POST");
        }
        var reply = replies[index];
        if (Docs.Text(reply, "requireUrl") is { Length: > 0 } wanted && wanted != url.AbsoluteUri)
        {
            return Status(ticket, 404);
        }
        await Task.Delay(TimeSpan.FromMilliseconds(Docs.Number(reply, "delayMs")), cancel);
        Record(url, ask.Text(4));
        return Status(ticket, (int)Docs.Number(reply, "status"), new SliceWrite(ask.Text(7), Docs.Text(reply, "body")));
    }

    void Record(Uri url, string body)
    {
        var line = Docs.Object(writer =>
        {
            writer.WriteString("body", body);
            writer.WriteString("url", url.AbsoluteUri);
        });
        lock (replies)
        {
            File.AppendAllText(requests, line + "\n");
        }
    }

    // A reply of `status` with no headers, as the real transport's receipt spells it.
    static Completion Status(string ticket, int status, params SliceWrite[] writes) =>
        Completion.Done(ticket, Network.Receipt(status, []), writes);
}
