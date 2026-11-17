using System.Collections.Concurrent;
using System.Diagnostics;
using System.Text;

namespace Osprey.Host;

/// <summary>
/// The four long-lived process capabilities [UI-EFFECTS-PROCESS]: every child under the name the
/// application gave it, a start replacing a child of the same name, and every child stopped when the
/// host ends. Fields: process-start command 1, arguments 2, environment 3, name 4; process-write
/// name 1, input 2; process-read name 1, limit 2, deadline 3; process-stop name 1.
/// </summary>
public sealed class ChildPool
{
    /// <summary>The most bytes of each stream a child keeps unread.</summary>
    public const int BufferCap = 1_000_000;

    /// <summary>The kinds this pool answers.</summary>
    public static IReadOnlyList<string> Kinds { get; } = ["process-start", "process-write", "process-read", "process-stop"];

    readonly ConcurrentDictionary<string, Child> children = new();
    readonly ConcurrentDictionary<string, Child> sessions = new();
    readonly ConcurrentDictionary<string, byte> exchanging = new();
    readonly Lock sessionSlots = new();

    /// <summary>A pool whose children end with the host, however the host ends.</summary>
    public ChildPool() => AppDomain.CurrentDomain.ProcessExit += (_, _) => StopAll();

    /// <summary>Answers one process request off the UI thread; a failure is the application's error, never a crash.</summary>
    public Task<Completion> Answer(Ask ask) => Task.Run(async () =>
    {
        try
        {
            return Completion.Done(ask.Ticket, await Result(ask));
        }
        catch (Exception failure) when (failure is HostException or IOException or InvalidOperationException or System.ComponentModel.Win32Exception)
        {
            return Completion.Failed(ask.Ticket, failure.Message);
        }
    });

    Task<string> Result(Ask ask) => ask.Kind() switch
    {
        "process-start" => Task.FromResult(Started(ask)),
        "process-write" => Written(ask),
        "process-read" => Read(ask),
        _ => Task.FromResult(Docs.Object(writer => writer.WriteBoolean("stopped", Stop(ask.Text(1))))),
    };

    string Started(Ask ask)
    {
        var child = Child.Start(ask.Text(1), ask.Texts(2), ask.Pairs(3), BufferCap);
        var name = ask.Text(4);
        children.AddOrUpdate(name, child, (_, previous) =>
        {
            previous.Stop();
            return child;
        });
        return Docs.Object(writer => writer.WriteNumber("pid", child.Id));
    }

    async Task<string> Written(Ask ask)
    {
        var written = await Named(ask.Text(1)).Write(ask.Text(2));
        return Docs.Object(writer => writer.WriteNumber("written", written));
    }

    async Task<string> Read(Ask ask)
    {
        var limit = (int)Math.Clamp(ask.Number(2), 0, BufferCap);
        var reading = await Named(ask.Text(1)).Read(limit, TimeSpan.FromMilliseconds(Math.Max(0, ask.Number(3))));
        return Docs.Object(writer =>
        {
            writer.WriteString("output", reading.Output);
            writer.WriteString("stderr", reading.Errors);
            writer.WriteBoolean("exited", reading.Exited);
            writer.WriteNumber("status", reading.Status);
        });
    }

    Child Named(string name) =>
        children.TryGetValue(name, out var child) ? child : throw new HostException($"No process named {name}");

    /// <summary>Open, exchange one line with, or close a bounded process session [SYNTH-HOST-STREAM-SESSION].</summary>
    public Task<Completion> Session(Ask ask) => Task.Run(async () =>
    {
        try
        {
            var operation = ask.Text(2);
            var fields = SessionFields(ask, operation);
            return operation switch
            {
                "session-open" => OpenSession(ask.Ticket, fields),
                "session-exchange" => await ExchangeSession(ask, Value(fields, "session"), Value(fields, "stdinText")),
                _ => CloseSession(ask.Ticket, Value(fields, "session")),
            };
        }
        catch (Exception failure) when (failure is HostException or IOException or TimeoutException or InvalidOperationException or System.ComponentModel.Win32Exception)
        {
            return Completion.Failed(ask.Ticket, failure.Message);
        }
    });

    static IReadOnlyList<(string Name, string Value)> SessionFields(Ask ask, string operation)
    {
        string[] allowed = operation switch
        {
            "session-open" => ["executable", "argv"],
            "session-exchange" => ["session", "stdinText"],
            "session-close" => ["session"],
            _ => throw new HostException("Unsupported process session operation"),
        };
        var fields = ask.Pairs(3);
        if (ask.Texts(3).Count % 2 != 0 || fields.Any(field => !allowed.Contains(field.Name)) ||
            fields.Where(field => field.Name != "argv").GroupBy(field => field.Name).Any(group => group.Count() > 1) ||
            allowed.Where(name => name != "argv").Any(name => !fields.Any(field => field.Name == name)))
            throw new HostException("Invalid process session argument pairs");
        return fields;
    }

    static string Value(IReadOnlyList<(string Name, string Value)> fields, string name) =>
        fields.FirstOrDefault(field => field.Name == name).Value ?? "";

    Completion OpenSession(string ticket, IReadOnlyList<(string Name, string Value)> fields)
    {
        var command = Value(fields, "executable");
        if (!Path.IsPathFullyQualified(command)) throw new HostException("Executable must be an absolute path");
        var key = $"process-{Guid.NewGuid():N}";
        lock (sessionSlots)
        {
            if (sessions.Count >= 16) throw new HostException("All 16 process session slots are occupied");
            sessions[key] = Child.Start(command, fields.Where(field => field.Name == "argv").Select(field => field.Value), [], BufferCap);
        }
        return Completion.Done(ticket, Docs.Object(writer => { writer.WriteNumber("status", 0); writer.WriteString("session", key); }));
    }

    async Task<Completion> ExchangeSession(Ask ask, string key, string input)
    {
        var timeout = ask.Number(4);
        var limit = ask.Number(5);
        if (timeout is < 1 or > 60_000 || limit is < 1 or > 4_194_304)
            throw new HostException("Invalid process session limits");
        if (input.Contains('\r') || input.Contains('\n') || Encoding.UTF8.GetByteCount(input) > 2_097_152)
            throw new HostException("Session input must be one line of at most 2 MiB");
        if (!exchanging.TryAdd(key, 0)) throw new HostException("Process session is busy");
        try
        {
            if (!sessions.TryGetValue(key, out var child)) throw new HostException("Unknown process session");
            return await Exchange(ask, input, child, TimeSpan.FromMilliseconds(timeout), (int)limit);
        }
        catch
        {
            if (sessions.TryRemove(key, out var child)) child.Kill();
            throw;
        }
        finally { exchanging.TryRemove(key, out _); }
    }

    static async Task<Completion> Exchange(Ask ask, string input, Child child, TimeSpan timeout, int limit)
    {
        var clock = Stopwatch.StartNew();
        await child.Write(input + "\n").WaitAsync(timeout);
        var output = new StringBuilder();
        var errors = new StringBuilder();
        int bytes = 0;
        while (clock.Elapsed < timeout)
        {
            var piece = await child.Read(Math.Min(limit + 1, BufferCap), timeout - clock.Elapsed);
            bytes += Encoding.UTF8.GetByteCount(piece.Output);
            if (bytes > limit + 2) throw new HostException("Process session reply exceeds its limit");
            output.Append(piece.Output);
            if (errors.Length + piece.Errors.Length <= 65_536) errors.Append(piece.Errors);
            if (piece.Output.EndsWith('\n')) return Replied(ask, output.ToString(), errors.ToString(), limit);
            if (piece.Exited) throw new HostException($"Process session exited with status {piece.Status}: {errors}");
        }
        throw new HostException($"Process session timed out: {errors}");
    }

    static Completion Replied(Ask ask, string text, string errors, int limit)
    {
        var line = text[..^1];
        if (line.EndsWith('\r')) line = line[..^1];
        if (line.Contains('\n') || line.Contains('\r') || Encoding.UTF8.GetByteCount(line) > limit)
            throw new HostException("Process session returned an invalid line");
        var receipt = Docs.Object(writer =>
        {
            writer.WriteNumber("status", 0);
            writer.WriteString("stderr", errors);
            writer.WriteString("output", line);
        });
        return Completion.Done(ask.Ticket, receipt);
    }

    Completion CloseSession(string ticket, string key)
    {
        if (!exchanging.TryAdd(key, 0)) throw new HostException("Process session is busy");
        try { if (sessions.TryRemove(key, out var child)) child.Kill(); }
        finally { exchanging.TryRemove(key, out _); }
        return Completion.Done(ticket, "{\"status\":0}");
    }

    /// <summary>Stops one child; an unknown or finished name is a harmless false.</summary>
    public bool Stop(string name)
    {
        if (!children.TryRemove(name, out var child) || !child.Running)
        {
            return false;
        }
        child.Stop();
        return true;
    }

    // Kills every child, for the host's end: nothing later could finish a graceful stop.
    void StopAll()
    {
        foreach (var key in sessions.Keys)
            if (sessions.TryRemove(key, out var session)) session.Kill();
        foreach (var name in children.Keys)
        {
            if (children.TryRemove(name, out var child))
            {
                child.Kill();
            }
        }
    }
}
