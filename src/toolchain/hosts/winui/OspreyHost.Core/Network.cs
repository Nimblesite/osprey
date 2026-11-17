using System.Net;
using System.Text;

namespace Osprey.Host;

/// <summary>
/// The <c>http</c> capability [UI-EFFECTS] [AGENT-PROVIDERS]: HTTPS, or HTTP on this machine, with
/// no redirects, cookies or cache, the request's own deadline and a streamed response limit. Nothing
/// is retried or written partially. The request's fields: method 1, URL 2, headers 3, body 4, limit 5,
/// deadline 6, slice 7. The receipt is the status and the response's headers sorted by name; the
/// body is written to the slice.
/// </summary>
public static class Network
{
    const int MaxHeaders = 100;
    const int MaxHeaderBytes = 16384;
    const int Block = 16384;

    static readonly HttpClient Client = new(new SocketsHttpHandler
    {
        AllowAutoRedirect = false,
        UseCookies = false,
        AutomaticDecompression = DecompressionMethods.All,
    })
    { Timeout = Timeout.InfiniteTimeSpan };

    /// <summary>Plain HTTP stays on this machine: nothing leaves it unencrypted, and local servers are reachable.</summary>
    public static bool IsLoopback(Uri url) =>
        url.Scheme == Uri.UriSchemeHttp && url.DnsSafeHost is "localhost" or "127.0.0.1" or "::1";

    /// <summary>The URL the request names when the host may reach it, else the reason it may not.</summary>
    public static (Uri? Url, string Problem) Allowed(string text) =>
        Uri.TryCreate(text, UriKind.Absolute, out var url) && (url.Scheme == Uri.UriSchemeHttps && url.Host.Length > 0 || IsLoopback(url))
            ? (url, "")
            : (null, "Only https URLs, or http on this machine, are allowed");

    /// <summary>Sends the request and completes with its receipt, or fails with why; <paramref name="cancel"/> is the app's cancel.</summary>
    public static async Task<Completion> Fetch(string ticket, Ask ask, CancellationToken cancel)
    {
        var (url, problem) = Allowed(ask.Text(2));
        if (url is null)
        {
            return Completion.Failed(ticket, problem);
        }
        var deadline = Math.Max(1, ask.Number(6));
        using var timer = CancellationTokenSource.CreateLinkedTokenSource(cancel);
        timer.CancelAfter(TimeSpan.FromMilliseconds(deadline));
        try
        {
            using var request = Outgoing(url, ask);
            return await Received(ticket, ask, request, timer.Token);
        }
        catch (OperationCanceledException)
        {
            return Completion.Failed(ticket, cancel.IsCancellationRequested ? "cancelled" : $"The request ran past its deadline of {deadline} ms");
        }
        catch (Exception failure) when (failure is HttpRequestException or IOException or InvalidOperationException or FormatException or HostException)
        {
            return Completion.Failed(ticket, failure.Message);
        }
    }

    /// <summary>The request the ask describes: its method, its headers on the request or its body, and its body when it has one.</summary>
    public static HttpRequestMessage Outgoing(Uri url, Ask ask)
    {
        var request = new HttpRequestMessage(new HttpMethod(ask.Text(1)), url);
        var body = ask.Text(4);
        if (body.Length > 0)
        {
            request.Content = new ByteArrayContent(Encoding.UTF8.GetBytes(body));
        }
        foreach (var (name, value) in ask.Pairs(3))
        {
            if (!request.Headers.TryAddWithoutValidation(name, value) && request.Content?.Headers.TryAddWithoutValidation(name, value) != true)
            {
                throw new HostException($"The header {name} cannot be sent");
            }
        }
        return request;
    }

    static async Task<Completion> Received(string ticket, Ask ask, HttpRequestMessage request, CancellationToken token)
    {
        using var response = await Client.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, token);
        var receipt = Receipt((int)response.StatusCode, Headers(response));
        var body = await Bounded(await response.Content.ReadAsStreamAsync(token), Math.Max(0, ask.Number(5)), token);
        return Completion.Done(ticket, receipt, new SliceWrite(ask.Text(7), body));
    }

    /// <summary>A receipt of <paramref name="status"/> and <paramref name="headers"/>, as the application reads it.</summary>
    public static string Receipt(int status, IReadOnlyList<(string Name, string Value)> headers) => Docs.Object(writer =>
    {
        writer.WriteNumber("status", status);
        writer.WriteStartArray("headers");
        foreach (var (name, value) in headers)
        {
            writer.WriteStartObject();
            writer.WriteString("name", name);
            writer.WriteString("value", value);
            writer.WriteEndObject();
        }
        writer.WriteEndArray();
    });

    // Every response and content header, sorted by name, within the receipt's count and size.
    static List<(string Name, string Value)> Headers(HttpResponseMessage response)
    {
        IEnumerable<KeyValuePair<string, IEnumerable<string>>> all = response.Content.Headers.Concat(response.Headers);
        var headers = all.Select(header => (header.Key, string.Join(", ", header.Value))).OrderBy(header => header.Key, StringComparer.Ordinal).ToList();
        var bytes = headers.Sum(header => Encoding.UTF8.GetByteCount(header.Key) + Encoding.UTF8.GetByteCount(header.Item2));
        if (headers.Count > MaxHeaders || bytes > MaxHeaderBytes)
        {
            throw new HostException($"The response's headers passed their limit of {MaxHeaders} headers or {MaxHeaderBytes} bytes");
        }
        return headers;
    }

    /// <summary>The whole stream as UTF-8 text, failing once it passes <paramref name="limit"/> bytes or carries a NUL.</summary>
    public static async Task<string> Bounded(Stream stream, long limit, CancellationToken token)
    {
        using var kept = new MemoryStream();
        var block = new byte[Block];
        int read;
        while ((read = await stream.ReadAsync(block, token)) > 0)
        {
            if (kept.Length + read > limit)
            {
                throw new HostException($"The response passed its limit of {limit} bytes");
            }
            kept.Write(block, 0, read);
        }
        var text = Encoding.UTF8.GetString(kept.GetBuffer(), 0, (int)kept.Length);
        return text.Contains('\0') ? throw new HostException("The response contains a NUL character") : text;
    }
}
