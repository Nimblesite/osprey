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

    Completion Read(string ticket, JsonElement request) => Existing(ticket, request, (name, path) =>
    {
        var limit = Envelopes.Number(request, "limit");
        return new FileInfo(path).Length > limit
            ? Completion.Failed(ticket, $"store entry {name} exceeds limit {limit}")
            : Completion.Done(ticket, "null", new SliceWrite(Envelopes.Text(request, "slice"), Strict.GetString(File.ReadAllBytes(path))));
    });

    Completion Write(string ticket, JsonElement request) => Named(ticket, request, (_, path) =>
    {
        if (Slices.Get(Envelopes.Text(request, "slice")) is not { } text)
        {
            return Completion.Failed(ticket, "store-write slice is missing");
        }
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        var temporary = path + ".writing";
        File.WriteAllText(temporary, text, Strict);
        File.Move(temporary, path, overwrite: true);
        return Completion.Done(ticket, "null");
    });

    Completion List(string ticket, JsonElement request) => Named(ticket, request, (_, folder) => Directory.Exists(folder)
        ? Completion.Done(ticket, Json.Array(Directory.EnumerateFileSystemEntries(folder).Select(Path.GetFileName).Order(StringComparer.Ordinal)!))
        : Completion.Done(ticket, "[]"), folder: true);

    Completion Delete(string ticket, JsonElement request) => Existing(ticket, request, (_, path) =>
    {
        File.Delete(path);
        return Completion.Done(ticket, "null");
    });

    // Answers with `act` on the requested name and its path when the name stays inside the store.
    Completion Named(string ticket, JsonElement request, Func<string, string, Completion> act, bool folder = false)
    {
        var name = Envelopes.Text(request, "name");
        return Entry(name, folder) is { } path ? act(name, path) : Completion.Failed(ticket, $"invalid store entry {name}");
    }

    // As Named, for an entry that must already be in the store.
    Completion Existing(string ticket, JsonElement request, Func<string, string, Completion> act) => Named(ticket, request, (name, path) =>
        File.Exists(path) ? act(name, path) : Completion.Failed(ticket, $"no such store entry {name}"));

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
