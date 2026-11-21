using System.Runtime.Versioning;
namespace Osprey.Host.Tests;

/// <summary>[WINUI-HOST-CAPABILITIES]: each adapter does what Osprey asked and reports it; a kind this host does not answer fails by name, and a chooser answers with the bare path.</summary>
[SupportedOSPlatform("windows")]
public sealed class CapabilitiesTests
{
    static Ask Asked(long code, string ticket, params AskField[] fields) => new(code, ticket, fields);

    static AskField Text(long field, string text) => new(field, 0, text);

    static AskField Number(long field, long number) => new(field, number, "");

    static Capabilities Host(string? chosen = null, List<string>? asked = null, string? saved = null) => new(
        new Pickers(
            () => Task.FromResult(chosen),
            types =>
            {
                asked?.AddRange(types);
                return Task.FromResult(chosen);
            },
            _ => Task.FromResult(saved)),
        new Store(Path.Combine(Path.GetTempPath(), "osprey-store-unused")));

    [Fact]
    public async Task AKindThisHostDoesNotAnswerFailsByName()
    {
        foreach (var (kind, at) in Framework.CapabilityKinds.Select((kind, at) => (kind, at)))
        {
            if (Coverage.Answered.Contains(kind))
            {
                continue;
            }
            var done = await Host().Answer(Asked(at + 1, "t"));
            Assert.False(done.Ok);
            Assert.Contains(kind, done.Error, StringComparison.Ordinal);
        }
    }

    [Fact]
    public async Task AChosenFolderComesBackWithItsPath()
    {
        var done = await Host("D:\\Media").Answer(Asked(14, "choose"));
        Assert.Equal(("choose", true, "D:\\Media"), (done.Ticket, done.Ok, done.Result));
    }

    [Fact]
    public async Task AChosenFolderComesBackWithItsPathExtraUnreadFieldsChangeNothing()
    {
        var done = await Host("E:\\Zoë's Folder").Answer(Asked(14, "choose", Number(2, 5), Text(3, "ignored")));
        Assert.Equal(("choose", true, "E:\\Zoë's Folder"), (done.Ticket, done.Ok, done.Result));
        Assert.Empty(done.Writes);
    }

    [Fact]
    public async Task ADismissedPickerIsACancelledChoice()
    {
        var done = await Host(null).Answer(Asked(14, "choose"));
        Assert.Equal(("", true), (done.Result, done.Ok));
    }

    [Fact]
    public async Task ADismissedPickerIsACancelledChoiceExtraTheFilePickerStillOfferedItsTypes()
    {
        var asked = new List<string>();
        var done = await Host(null, asked).Answer(Asked(13, "choose", Text(1, "mp3"), Text(1, "wav")));
        Assert.Equal(["mp3", "wav"], asked);
        Assert.Equal((true, ""), (done.Ok, done.Result));
    }

    [Fact]
    public async Task AChosenFileComesBackWithItsPathAndThePickerOffersOnlyTheTypesAsked()
    {
        var asked = new List<string>();
        var done = await Host("D:\\Music\\Sunroom.mp3", asked).Answer(Asked(13, "choose", Text(1, "mp3"), Number(2, 0), Text(3, "")));
        Assert.Equal(("choose", "D:\\Music\\Sunroom.mp3"), (done.Ticket, done.Result));
        Assert.Equal(["mp3"], asked);
        var dismissed = await Host(null).Answer(Asked(13, "choose"));
        Assert.Equal("", dismissed.Result);
        Assert.True(dismissed.Ok);
    }

    [Fact]
    public async Task AChosenFileComesBackWithItsPathAndThePickerOffersOnlyTheTypesAskedExtraSeveralTypesInOrder()
    {
        var asked = new List<string>();
        var done = await Host("D:\\Music\\Both.flac", asked).Answer(Asked(13, "choose", Text(1, "mp3"), Text(1, "wav"), Text(1, "flac"), Number(2, 3)));
        Assert.Equal((true, "D:\\Music\\Both.flac"), (done.Ok, done.Result));
        Assert.Equal(["mp3", "wav", "flac"], asked);
    }

    [Fact]
    public async Task ASaveWritesTheStagedTextAndCompletesWithTheChosenPath()
    {
        var target = Path.Combine(Path.GetTempPath(), $"osprey-save-{Guid.NewGuid():N}.txt");
        try
        {
            Slices.Put("staged", "the saved text");
            var done = await Host(saved: target).Answer(Asked(15, "save", Text(1, "note.txt"), Text(2, "staged")));
            Assert.Equal((true, target), (done.Ok, done.Result));
            Assert.Equal("the saved text", File.ReadAllText(target));
            var dismissed = await Host(null).Answer(Asked(15, "save", Text(1, "note.txt"), Text(2, "staged")));
            Assert.Equal((true, ""), (dismissed.Ok, dismissed.Result));
        }
        finally
        {
            File.Delete(target);
        }
    }

    [Fact]
    public async Task ASaveWhoseSliceWasNeverHeldFails()
    {
        var done = await Host(saved: Path.GetTempPath()).Answer(Asked(15, "save", Text(1, "x.txt"), Text(2, "never-written")));
        Assert.Equal((false, "save-file slice is missing"), (done.Ok, done.Error));
    }

    static Ask Run(string executable, long timeout, long limit, params string[] arguments)
    {
        var fields = new List<AskField> { Text(1, executable) };
        fields.AddRange(arguments.Select(argument => Text(2, argument)));
        fields.Add(Number(3, timeout));
        fields.Add(Number(4, limit));
        fields.Add(Number(5, 0));
        fields.Add(Text(6, "out"));
        return Asked(1, "run", [.. fields]);
    }

    [Fact]
    public async Task ARunWritesItsStdoutToTheSliceAndReportsItsStatus()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "run starts a Windows executable");
        var shell = Path.Combine(Environment.SystemDirectory, "cmd.exe");
        var done = await Host().Answer(Run(shell, 10_000, 65_536, "/c", "echo _googlecast._tcp SRV& exit 3"));
        var result = System.Text.Json.JsonDocument.Parse(done.Result);
        Assert.Equal((true, 3L), (done.Ok, Docs.Number(result.RootElement, "status")));
        Assert.Equal("out", done.Writes[0].Key);
        Assert.StartsWith("_googlecast._tcp SRV", done.Writes[0].Text, StringComparison.Ordinal);
        var relative = await Host().Answer(Run("cmd.exe", 10_000, 65_536));
        Assert.Equal((false, "Executable must be an absolute path"), (relative.Ok, relative.Error));
        var late = await Host().Answer(Run(shell, 200, 65_536, "/c", "ping -n 30 127.0.0.1"));
        Assert.Contains("deadline", late.Error, StringComparison.Ordinal);
        var loud = await Host().Answer(Run(shell, 10_000, 4, "/c", "echo more than four bytes"));
        Assert.Equal((false, "The output passed its limit"), (loud.Ok, loud.Error));
    }

    [Fact]
    public async Task CancellingAWalkThatIsNotRunningChangesNothing()
    {
        var done = await Host().Answer(Asked(19, "cancel:1", Text(1, "walk:9")));
        Assert.Equal((true, "null"), (done.Ok, done.Result));
    }

    [Fact]
    public async Task RevealingAPathThatDoesNotExistFails()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "reveal opens Windows File Explorer");
        var done = await Host().Answer(Asked(17, "reveal", Text(1, Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString("N")))));
        Assert.False(done.Ok);
        Assert.NotEmpty(done.Error);
    }

    [Fact]
    public async Task AWalkWritesTheSliceOspreyNamedAndCompletesWithNull()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "the folder walk reads Windows file ids and volumes");
        var folder = Directory.CreateTempSubdirectory("osprey-walk-");
        try
        {
            File.WriteAllBytes(Path.Combine(folder.FullName, "a.bin"), new byte[4096]);
            var done = await Host().Answer(Asked(16, "walk:1", Text(1, folder.FullName), Number(2, 100), Number(3, 60_000), Text(4, "walked")));
            Assert.True(done.Ok, done.Error);
            Assert.Equal("null", done.Result);
            var write = Assert.Single(done.Writes);
            Assert.Equal("walked", write.Key);
            using var slice = System.Text.Json.JsonDocument.Parse(write.Text);
            var walked = Assert.Single(Docs.Items(slice.RootElement, "folders", item => item));
            Assert.Equal(folder.FullName, Docs.Text(walked, "path"));
            var entry = Assert.Single(Docs.Items(walked, "entries", item => item));
            Assert.Equal("a.bin", Docs.Text(entry, "name"));
            Assert.True(Docs.Number(entry, "bytes") >= 4096);
        }
        finally
        {
            folder.Delete(recursive: true);
        }
    }

    // ---- http [UI-EFFECTS] against a real server on this machine ----

    static Ask Http(string ticket, string method, string url, string body = "", long limit = 65_536, long deadline = 10_000, params (string Name, string Value)[] headers)
    {
        var fields = new List<AskField> { Text(1, method), Text(2, url) };
        foreach (var (name, value) in headers)
        {
            fields.Add(Text(3, name));
            fields.Add(Text(3, value));
        }
        fields.AddRange([Text(4, body), Number(5, limit), Number(6, deadline), Text(7, "reply")]);
        return Asked(10, ticket, [.. fields]);
    }

    static System.Text.Json.JsonElement Receipt(Completion done) => System.Text.Json.JsonDocument.Parse(done.Result).RootElement;

    [Fact]
    public async Task AnHttpRequestCarriesItsMethodHeadersAndBodyAndItsReplyLandsInTheSlice()
    {
        await using var server = new Loopback(_ => Task.FromResult(Loopback.Reply("200 OK", "{\"choices\":[]}", ("X-Zeta", "z"), ("X-Alpha", "a"))));
        var done = await Host().Answer(Http("chat", "POST", server.Url("/v1/chat"), "{\"model\":\"m\"}", headers: [("Authorization", "Bearer sk-test"), ("Content-Type", "application/json")]));
        Assert.True(done.Ok, done.Error);
        Assert.Equal(200, Docs.Number(Receipt(done), "status"));
        var names = Docs.Items(Receipt(done), "headers", header => Docs.Text(header, "name")).ToList();
        Assert.True(names.IndexOf("X-Alpha") >= 0 && names.IndexOf("X-Alpha") < names.IndexOf("X-Zeta"), string.Join(",", names));
        Assert.Equal(("reply", "{\"choices\":[]}"), (done.Writes[0].Key, done.Writes[0].Text));
        var heard = Assert.Single(server.Heard);
        Assert.StartsWith("POST /v1/chat HTTP/1.1", heard, StringComparison.Ordinal);
        Assert.Contains("Authorization: Bearer sk-test", heard, StringComparison.Ordinal);
        Assert.Contains("Content-Type: application/json", heard, StringComparison.Ordinal);
        Assert.EndsWith("{\"model\":\"m\"}", heard, StringComparison.Ordinal);
    }

    [Fact]
    public async Task AnHttpReplyPastItsLimitOrDeadlineFailsAndARedirectIsReportedNotFollowed()
    {
        await using var server = new Loopback(async request =>
        {
            if (request.Contains("/slow", StringComparison.Ordinal))
            {
                await Task.Delay(3000);
            }
            return request.Contains("/moved", StringComparison.Ordinal)
                ? Loopback.Reply("302 Found", "", ("Location", "https://example.test/elsewhere"))
                : Loopback.Reply("200 OK", "twenty bytes of body");
        });
        var loud = await Host().Answer(Http("a", "GET", server.Url("/big"), limit: 4));
        Assert.False(loud.Ok);
        Assert.Contains("passed its limit of 4 bytes", loud.Error, StringComparison.Ordinal);
        var late = await Host().Answer(Http("b", "GET", server.Url("/slow"), deadline: 200));
        Assert.Contains("deadline of 200 ms", late.Error, StringComparison.Ordinal);
        var moved = await Host().Answer(Http("c", "GET", server.Url("/moved")));
        Assert.Equal((true, 302L), (moved.Ok, Docs.Number(Receipt(moved), "status")));
        Assert.Equal(3, server.Heard.Count);
        foreach (var refused in new[] { "http://example.test/v1", "ftp://127.0.0.1/x", "not a url", "https:///nohost" })
        {
            var done = await Host().Answer(Http("r", "GET", refused));
            Assert.Equal((false, "Only https URLs, or http on this machine, are allowed"), (done.Ok, done.Error));
        }
    }

    [Fact]
    public async Task ACancelStopsAnHttpRequestInFlight()
    {
        await using var server = new Loopback(async _ =>
        {
            await Task.Delay(5000);
            return Loopback.Reply("200 OK", "late");
        });
        var host = Host();
        var waiting = host.Answer(Http("slow:1", "GET", server.Url("/slow")));
        await Task.Delay(300, TestContext.Current.CancellationToken);
        var cancelled = await host.Answer(Asked(19, "cancel:2", Text(1, "slow:1")));
        Assert.True(cancelled.Ok);
        var done = await waiting;
        Assert.Equal((false, "cancelled"), (done.Ok, done.Error));
    }

    // ---- long-lived processes [UI-EFFECTS-PROCESS] ----

    static Ask Process(long code, string ticket, params AskField[] fields) => Asked(code, ticket, fields);

    [Fact]
    public async Task AProcessSessionExchangesBoundedLinesAndClosesItsWorker()
    {
        var host = Host();
        var windows = OperatingSystem.IsWindows();
        var executable = windows ? Path.Combine(Environment.SystemDirectory, @"WindowsPowerShell\v1.0\powershell.exe") : "/bin/cat";
        const string echo = "while ($null -ne ($line = [Console]::ReadLine())) { [Console]::WriteLine($line) }";
        AskField[] arguments = windows
            ? [Text(3, "argv"), Text(3, "-NoProfile"), Text(3, "argv"), Text(3, "-NonInteractive"), Text(3, "argv"), Text(3, "-Command"), Text(3, "argv"), Text(3, echo)]
            : [];
        var opened = await host.Answer(Process(11, "open",
            [Text(1, "process"), Text(2, "session-open"), Text(3, "executable"), Text(3, executable), .. arguments]));
        Assert.True(opened.Ok, opened.Error);
        var key = Docs.Text(Receipt(opened), "session");
        Assert.NotEmpty(key);
        Ask Exchange(string input, long limit = 100) => Process(11, "exchange", Text(1, "process"), Text(2, "session-exchange"),
            Text(3, "session"), Text(3, key), Text(3, "stdinText"), Text(3, input), Number(4, 5_000), Number(5, limit));
        foreach (var line in new[] { "first", "second" })
        {
            var reply = await host.Answer(Exchange(line));
            Assert.True(reply.Ok, reply.Error);
            Assert.Equal(line, Docs.Text(Receipt(reply), "output"));
        }
        Assert.False((await host.Answer(Exchange("echo ignored\necho extra"))).Ok);
        Assert.False((await host.Answer(Exchange("echo too long", 2))).Ok);
        Assert.False((await host.Answer(Exchange("echo closed"))).Ok);
        var closed = await host.Answer(Process(11, "close", Text(1, "process"), Text(2, "session-close"), Text(3, "session"), Text(3, key)));
        Assert.True(closed.Ok, closed.Error);
    }

    // Reads the child named `name` until `done` holds for all its output so far and whether it has exited.
    static async Task<(string Output, bool Exited, long Status)> ReadUntil(Capabilities host, string name, Func<string, bool, bool> done)
    {
        var output = "";
        for (var tries = 0; tries < 100; tries++)
        {
            var read = await host.Answer(Process(24, "read", Text(1, name), Number(2, 4096), Number(3, 200)));
            Assert.True(read.Ok, read.Error);
            var result = Receipt(read);
            output += Docs.Text(result, "output");
            if (done(output, Docs.Flag(result, "exited")))
            {
                return (output, Docs.Flag(result, "exited"), Docs.Number(result, "status"));
            }
        }
        throw new Xunit.Sdk.XunitException($"{name} never answered; it wrote {output}");
    }

    [Fact]
    public async Task AChildTakesInputWritesOutputSeesItsEnvironmentAndReportsItsExit()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "the child is cmd.exe");
        var host = Host();
        var started = await host.Answer(Process(22, "start", Text(1, "cmd"), Text(2, "/d"), Text(2, "/q"), Text(3, "OSPREY_PROBE"), Text(3, "probe-42"), Text(4, "shell")));
        Assert.True(started.Ok, started.Error);
        Assert.True(Docs.Number(Receipt(started), "pid") > 0);
        var written = await host.Answer(Process(23, "write", Text(1, "shell"), Text(2, "echo heard-%OSPREY_PROBE%\r\n")));
        Assert.Equal(("echo heard-%OSPREY_PROBE%\r\n".Length, true), (Docs.Number(Receipt(written), "written"), written.Ok));
        var heard = await ReadUntil(host, "shell", (output, _) => output.Contains("heard-probe-42", StringComparison.Ordinal));
        Assert.False(heard.Exited);
        await host.Answer(Process(23, "write", Text(1, "shell"), Text(2, "exit 7\r\n")));
        var ended = await ReadUntil(host, "shell", (_, exited) => exited);
        Assert.Equal((true, 7L), (ended.Exited, ended.Status));
        var late = await host.Answer(Process(23, "write", Text(1, "shell"), Text(2, "echo too late\r\n")));
        Assert.False(late.Ok);
        Assert.Contains("has exited", late.Error, StringComparison.Ordinal);
    }

    [Fact]
    public async Task AStoppedChildEndsAndUnknownOrMissingNamesFailByName()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "the child is cmd.exe");
        var host = Host();
        Assert.True((await host.Answer(Process(22, "start", Text(1, Path.Combine(Environment.SystemDirectory, "cmd.exe")), Text(2, "/d"), Text(2, "/q"), Text(4, "server")))).Ok);
        var stopped = await host.Answer(Process(25, "stop", Text(1, "server")));
        Assert.True(Docs.Flag(Receipt(stopped), "stopped"));
        var again = await host.Answer(Process(25, "stop", Text(1, "server")));
        Assert.False(Docs.Flag(Receipt(again), "stopped"));
        var unknown = await host.Answer(Process(24, "read", Text(1, "never"), Number(2, 10), Number(3, 0)));
        Assert.Equal((false, "No process named never"), (unknown.Ok, unknown.Error));
        var missing = await host.Answer(Process(22, "start", Text(1, "osprey-no-such-command"), Text(4, "x")));
        Assert.Equal((false, "Command not found: osprey-no-such-command"), (missing.Ok, missing.Error));
        var absent = await host.Answer(Process(22, "start", Text(1, @"C:\no\such\tool.exe"), Text(4, "y")));
        Assert.Equal((false, @"Command not found: C:\no\such\tool.exe"), (absent.Ok, absent.Error));
    }

    [Fact]
    public void AReadNeverCutsACharacterUnlessTheChildHasEnded()
    {
        byte[] accented = [0x61, 0xC3, 0xA9], smile = [0xF0, 0x9F, 0x98, 0x80];
        Assert.Equal((1, 3, 1), (Child.Whole(accented, 2, false), Child.Whole(accented, 3, false), Child.Whole(accented, 2, true)));
        Assert.Equal((0, 4, 3), (Child.Whole(smile, 3, false), Child.Whole(smile, 9, false), Child.Whole(smile[..3], 3, true)));
        Assert.Equal(0, Child.Whole([], 8, false));
        if (OperatingSystem.IsWindows())
        {
            Assert.EndsWith("cmd.exe", Child.Resolve("cmd", Environment.SystemDirectory), StringComparison.OrdinalIgnoreCase);
            // MSYS2's usr\bin holds an extensionless bash script named cmd. Windows never runs a file
            // without an executable extension, so a lookup passes over it to the real cmd.exe.
            var msys = Directory.CreateTempSubdirectory("osprey-msys-");
            try
            {
                File.WriteAllText(Path.Combine(msys.FullName, "cmd"), "#!/usr/bin/env bash\n\"$COMSPEC\" \"$@\"\n");
                var path = msys.FullName + Path.PathSeparator + Environment.SystemDirectory;
                Assert.Equal(Path.Combine(Environment.SystemDirectory, "cmd.exe"), Child.Resolve("cmd", path), ignoreCase: true);
                Assert.Throws<HostException>(() => Child.Resolve("cmd", msys.FullName));
            }
            finally
            {
                msys.Delete(true);
            }
        }
        else
        {
            Assert.EndsWith("sh", Child.Resolve("sh", Environment.GetEnvironmentVariable("PATH") ?? ""), StringComparison.Ordinal);
        }
    }

    // ---- the model double a self-test answers http with [WINUI-HOST-SELF-TEST] ----

    [Fact]
    public async Task TheDoubleAnswersInOrderRecordsWithoutTheKeyAndLeavesThisMachineReal()
    {
        var folder = Directory.CreateTempSubdirectory("osprey-double-");
        try
        {
            var file = Path.Combine(folder.FullName, "doubles.json");
            File.WriteAllText(file, """{"models": {"data": [{"id": "deepseek-chat"}]}, "replies": [{"status": 200, "body": "first"}, {"status": 200, "body": "second", "requireUrl": "https://api.example.test/v1/chat"}]}""");
            var requests = Path.Combine(folder.FullName, "requests.jsonl");
            var host = new Capabilities(new Pickers(() => Task.FromResult<string?>(null), _ => Task.FromResult<string?>(null), _ => Task.FromResult<string?>(null)), new Store(folder.FullName))
            {
                Http = new HttpDouble(file, requests).Answer,
            };
            (string, string) bearer = ("Authorization", "Bearer sk-secret");
            var first = await host.Answer(Http("1", "POST", "https://api.example.test/v1/chat", "{\"n\":1}", headers: bearer));
            Assert.Equal((true, 200L, "first"), (first.Ok, Docs.Number(Receipt(first), "status"), first.Writes[0].Text));
            var listed = await host.Answer(Http("m", "GET", "https://api.example.test/v1/models", headers: bearer));
            Assert.Contains("deepseek-chat", listed.Writes[0].Text, StringComparison.Ordinal);
            var elsewhere = await host.Answer(Http("2", "POST", "https://other.example.test/v1/chat", headers: bearer));
            Assert.Equal((true, 404L), (elsewhere.Ok, Docs.Number(Receipt(elsewhere), "status")));
            var spent = await host.Answer(Http("3", "POST", "https://api.example.test/v1/chat", headers: bearer));
            Assert.Equal((false, "The double has no reply for request 3"), (spent.Ok, spent.Error));
            var record = Assert.Single(File.ReadAllLines(requests));
            Assert.Equal("{\"body\":\"{\\\"n\\\":1}\",\"url\":\"https://api.example.test/v1/chat\"}", record);
            Assert.DoesNotContain("sk-secret", File.ReadAllText(requests), StringComparison.Ordinal);
            await using var local = new Loopback(_ => Task.FromResult(Loopback.Reply("200 OK", "real")));
            var real = await host.Answer(Http("local", "GET", local.Url("/v1/models")));
            Assert.Equal((true, "real"), (real.Ok, real.Writes[0].Text));
            var unauthorized = new HttpDouble(file, requests);
            var refused = await unauthorized.Answer("u", Http("u", "POST", "https://api.example.test/v1/chat"), CancellationToken.None);
            Assert.Equal((false, "The double takes only an authorized POST"), (refused.Ok, refused.Error));
        }
        finally
        {
            folder.Delete(recursive: true);
        }
    }
}

/// <summary>An HTTP/1.1 server on this machine answering one request per connection, keeping each request it was sent.</summary>
sealed class Loopback : IAsyncDisposable
{
    readonly System.Net.Sockets.TcpListener listener = new(System.Net.IPAddress.Loopback, 0);
    readonly Func<string, Task<string>> reply;
    readonly CancellationTokenSource stop = new();
    readonly Task serving;
    readonly List<string> heard = [];

    public Loopback(Func<string, Task<string>> reply)
    {
        this.reply = reply;
        listener.Start();
        serving = Task.Run(Serve);
    }

    /// <summary>Every request received, whole, in order.</summary>
    public IReadOnlyList<string> Heard
    {
        get
        {
            lock (heard)
            {
                return [.. heard];
            }
        }
    }

    public string Url(string path) => $"http://127.0.0.1:{((System.Net.IPEndPoint)listener.LocalEndpoint).Port}{path}";

    /// <summary>A whole response with <paramref name="status"/>, <paramref name="headers"/> and <paramref name="body"/>.</summary>
    public static string Reply(string status, string body, params (string Name, string Value)[] headers) =>
        $"HTTP/1.1 {status}\r\n{string.Concat(headers.Select(header => $"{header.Name}: {header.Value}\r\n"))}Content-Length: {System.Text.Encoding.UTF8.GetByteCount(body)}\r\nConnection: close\r\n\r\n{body}";

    async Task Serve()
    {
        while (!stop.IsCancellationRequested)
        {
            System.Net.Sockets.TcpClient client;
            try
            {
                client = await listener.AcceptTcpClientAsync(stop.Token);
            }
            catch (Exception ended) when (ended is OperationCanceledException or System.Net.Sockets.SocketException or ObjectDisposedException)
            {
                return;
            }
            _ = Task.Run(() => Answered(client));
        }
    }

    async Task Answered(System.Net.Sockets.TcpClient client)
    {
        using (client)
        {
            var stream = client.GetStream();
            var request = await Request(stream);
            lock (heard)
            {
                heard.Add(request);
            }
            try
            {
                await stream.WriteAsync(System.Text.Encoding.UTF8.GetBytes(await reply(request)));
            }
            catch (IOException)
            {
                // The client gave up first, as a cancelled or late request does.
            }
        }
    }

    static async Task<string> Request(System.Net.Sockets.NetworkStream stream)
    {
        var bytes = new List<byte>();
        var block = new byte[4096];
        while (true)
        {
            var read = await stream.ReadAsync(block);
            bytes.AddRange(block.AsSpan(0, read));
            var text = System.Text.Encoding.UTF8.GetString([.. bytes]);
            var end = text.IndexOf("\r\n\r\n", StringComparison.Ordinal);
            if (read == 0 || end >= 0 && bytes.Count >= end + 4 + Length(text[..end]))
            {
                return text;
            }
        }
    }

    static int Length(string head) =>
        head.Split("\r\n").Select(line => line.Split(':', 2)).Where(parts => parts.Length == 2 && parts[0].Trim().Equals("Content-Length", StringComparison.OrdinalIgnoreCase))
            .Select(parts => int.Parse(parts[1].Trim(), System.Globalization.CultureInfo.InvariantCulture)).FirstOrDefault();

    public async ValueTask DisposeAsync()
    {
        await stop.CancelAsync();
        listener.Stop();
        await serving;
        stop.Dispose();
    }
}
