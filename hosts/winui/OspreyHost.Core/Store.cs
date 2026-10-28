using System.Text;
using System.Text.Json;

namespace Osprey.Host;

/// <summary>
/// The application's private store [WINUI-HOST-CAPABILITIES]: files under one folder per
/// application, <c>%APPDATA%\Osprey\&lt;app&gt;</c> as the GPUI host keeps them. Names are relative
/// and never leave that folder; a read is bounded and UTF-8, and a write replaces its entry whole.
/// </summary>
public sealed class Store(string root)
{
    static readonly UTF8Encoding Strict = new(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);

    /// <summary>The store folder for the application compiled into <paramref name="library"/>.</summary>
    public static string RootFor(string library) =>
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "Osprey", Path.GetFileNameWithoutExtension(library));

    /// <summary>Answers one store request; anything outside the store or the protocol fails with a reason.</summary>
    public Completion Answer(string ticket, JsonElement request)
    {
        try
        {
            return Envelopes.Text(request, "kind") switch
            {
                "store-read" => Read(ticket, request),
                "store-write" => Write(ticket, request),
                "store-list" => List(ticket, request),
                "store-delete" => Delete(ticket, request),
                var kind => Completion.Failed(ticket, $"{kind} is not a store request"),
            };
        }
        catch (Exception problem) when (problem is IOException or UnauthorizedAccessException or DecoderFallbackException)
        {
            return Completion.Failed(ticket, problem.Message);
        }
    }

    Completion Read(string ticket, JsonElement request)
    {
        var name = Envelopes.Text(request, "name");
        var limit = Envelopes.Number(request, "limit");
        if (Entry(name) is not { } path)
        {
            return Refused(ticket, name);
        }
        if (!File.Exists(path))
        {
            return Completion.Failed(ticket, $"no such store entry {name}");
        }
        if (new FileInfo(path).Length > limit)
        {
            return Completion.Failed(ticket, $"store entry {name} exceeds limit {limit}");
        }
        return Completion.Done(ticket, "null", new SliceWrite(Envelopes.Text(request, "slice"), Strict.GetString(File.ReadAllBytes(path))));
    }

    Completion Write(string ticket, JsonElement request)
    {
        var name = Envelopes.Text(request, "name");
        if (Entry(name) is not { } path)
        {
            return Refused(ticket, name);
        }
        if (Slices.Get(Envelopes.Text(request, "slice")) is not { } text)
        {
            return Completion.Failed(ticket, "store-write slice is missing");
        }
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        var temporary = path + ".writing";
        File.WriteAllText(temporary, text, Strict);
        File.Move(temporary, path, overwrite: true);
        return Completion.Done(ticket, "null");
    }

    Completion List(string ticket, JsonElement request)
    {
        var name = Envelopes.Text(request, "name");
        if (Entry(name, folder: true) is not { } folder)
        {
            return Refused(ticket, name);
        }
        if (!Directory.Exists(folder))
        {
            return Completion.Done(ticket, "[]");
        }
        var names = Directory.EnumerateFileSystemEntries(folder).Select(Path.GetFileName).Order(StringComparer.Ordinal);
        return Completion.Done(ticket, Json.Array(names!));
    }

    Completion Delete(string ticket, JsonElement request)
    {
        var name = Envelopes.Text(request, "name");
        if (Entry(name) is not { } path)
        {
            return Refused(ticket, name);
        }
        if (!File.Exists(path))
        {
            return Completion.Failed(ticket, $"no such store entry {name}");
        }
        File.Delete(path);
        return Completion.Done(ticket, "null");
    }

    static Completion Refused(string ticket, string name) => Completion.Failed(ticket, $"invalid store entry {name}");

    // A name inside the store: relative, with no drive, root or parent folder in it. Only a
    // listing may name the store itself, with the empty name.
    string? Entry(string name, bool folder = false)
    {
        if (name.Length == 0)
        {
            return folder ? root : null;
        }
        var parts = name.Split(['/', '\\']);
        var safe = !Path.IsPathRooted(name) && !name.Contains(':') && parts.All(part => part is not ("" or ".."));
        return safe ? Path.Combine([root, .. parts]) : null;
    }
}
