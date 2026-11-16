using System.Collections.Concurrent;

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
        foreach (var name in children.Keys)
        {
            if (children.TryRemove(name, out var child))
            {
                child.Kill();
            }
        }
    }
}
