using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;

namespace Osprey.Host;

/// <summary>
/// The application's private store [WINUI-HOST-CAPABILITIES]: files under one folder per
/// application, <c>%APPDATA%\Osprey\&lt;app&gt;</c> as the GPUI host keeps them, or the package's
/// LocalState in a packaged host, the one folder its sandbox may write [WINUI-HOST-SANDBOX]. Names are relative
/// and never leave that folder; a read is bounded and UTF-8, and a write replaces its entry whole.
/// </summary>
public sealed partial class Store(string root)
{
    static readonly UTF8Encoding Strict = new(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);
    const uint ListDirectory = 0x0001;
    const uint OpenExisting = 3;
    const uint BackupSemantics = 0x02000000;
    const uint OpenReparsePoint = 0x00200000;

    /// <summary>The store folder for the application compiled into <paramref name="library"/>.</summary>
    public static string RootFor(string library) =>
#if WINDOWS
        Sandbox.Packaged()
            ? Path.Combine(Windows.Storage.ApplicationData.Current.LocalFolder.Path, "store")
            : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "Osprey", Path.GetFileNameWithoutExtension(library));
#else
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "Osprey", Path.GetFileNameWithoutExtension(library));
#endif

    /// <summary>Answers one store request; anything outside the store or the protocol fails with a reason.</summary>
    public Completion Answer(Ask ask)
    {
        var ticket = ask.Ticket;
        try
        {
            return ask.Kind() switch
            {
                "store-read" => Read(ask),
                "store-write" => Write(ask),
                "store-list" => List(ask),
                "store-delete" => Delete(ask),
                var kind => Completion.Failed(ticket, $"{kind} is not a store request"),
            };
        }
        catch (Exception problem) when (problem is IOException or UnauthorizedAccessException or DecoderFallbackException)
        {
            return Completion.Failed(ticket, problem.Message);
        }
    }

    Completion Read(Ask ask) => Existing(ask, (name, path) =>
    {
        var limit = ask.Number(2);
        return new FileInfo(path).Length > limit
            ? Completion.Failed(ask.Ticket, $"store entry {name} exceeds limit {limit}")
            : Completion.Done(ask.Ticket, "null", new SliceWrite(ask.Text(3), Strict.GetString(File.ReadAllBytes(path))));
    });

    Completion Write(Ask ask) => Named(ask, (name, _) =>
    {
        if (Slices.Get(ask.Text(2)) is not { } text)
        {
            return Completion.Failed(ask.Ticket, "store-write slice is missing");
        }
        WriteEntry(name, text);
        return Completion.Done(ask.Ticket, "null");
    });

    void WriteEntry(string name, string text)
    {
        var parts = name.Split(['/', '\\']);
        var folder = Path.GetFullPath(root);
        var directories = new Stack<SafeFileHandle>();
        try
        {
            foreach (var part in parts.AsSpan(0, parts.Length - 1))
            {
                directories.Push(PinDirectory(folder, FileShare.Read));
                folder = Path.Combine(folder, part);
            }
            directories.Push(PinDirectory(folder, FileShare.ReadWrite));
            Replace(folder, parts[^1], Strict.GetBytes(text));
        }
        finally
        {
            while (directories.TryPop(out var directory)) directory.Dispose();
        }
    }

    // [WINUI-HOST-CAPABILITIES] Hold each directory without delete sharing, so it cannot be
    // renamed, removed or replaced by a junction during a write. The folders above the entry
    // also refuse write sharing, so an empty one cannot be turned into a junction in place.
    // The entry's own folder must share writing: moving the finished file into it opens the
    // folder for writing, and a refusal there is a sharing violation on every write.
    static SafeFileHandle PinDirectory(string path, FileShare share)
    {
        Directory.CreateDirectory(path);
        var handle = CreateFileFromAppW(path, ListDirectory, share, 0, OpenExisting, BackupSemantics | OpenReparsePoint, 0);
        try
        {
            if (handle.IsInvalid) throw new IOException(new Win32Exception(Marshal.GetLastPInvokeError()).Message);
            var attributes = File.GetAttributes(handle);
            if ((attributes & (FileAttributes.Directory | FileAttributes.ReparsePoint)) != FileAttributes.Directory)
                throw new IOException($"invalid store directory {path}");
            return handle;
        }
        catch
        {
            handle.Dispose();
            throw;
        }
    }

    static void Replace(string folder, string leaf, byte[] bytes)
    {
        var temporary = Path.Combine(folder, $".osprey-writing-{Guid.NewGuid():N}");
        var file = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None);
        var moved = false;
        try
        {
            using (file) file.Write(bytes);
            File.Move(temporary, Path.Combine(folder, leaf), overwrite: true);
            moved = true;
        }
        finally
        {
            if (!moved) File.Delete(temporary);
        }
    }

    [LibraryImport("api-ms-win-core-file-fromapp-l1-1-0.dll", SetLastError = true, StringMarshalling = StringMarshalling.Utf16)]
    private static partial SafeFileHandle CreateFileFromAppW(
        string path, uint access, FileShare share, nint security, uint creation, uint flags, nint template);

    Completion List(Ask ask) => Named(ask, (_, folder) => Directory.Exists(folder)
        ? Completion.Done(ask.Ticket, Docs.Array(Directory.EnumerateFileSystemEntries(folder).Select(path => Path.GetFileName(path) ?? "").Order(StringComparer.Ordinal)))
        : Completion.Done(ask.Ticket, "[]"), folder: true);

    Completion Delete(Ask ask) => Existing(ask, (_, path) =>
    {
        File.Delete(path);
        return Completion.Done(ask.Ticket, "null");
    });

    // Answers with `act` on the requested name and its path when the name stays inside the store.
    Completion Named(Ask ask, Func<string, string, Completion> act, bool folder = false)
    {
        var name = ask.Text(1);
        return Entry(name, folder) is { } path ? act(name, path) : Completion.Failed(ask.Ticket, $"invalid store entry {name}");
    }

    // As Named, for an entry that must already be in the store.
    Completion Existing(Ask ask, Func<string, string, Completion> act) => Named(ask, (name, path) =>
        File.Exists(path) ? act(name, path) : Completion.Failed(ask.Ticket, $"no such store entry {name}"));

    // A name inside the store: relative, with no drive, root or parent folder in it. Only a
    // listing may name the store itself, with the empty name.
    string? Entry(string name, bool folder = false)
    {
        if (name.Length == 0)
        {
            return folder ? root : null;
        }
        var parts = name.Split(['/', '\\']);
        var safe = !Path.IsPathRooted(name) && parts.All(part =>
            part.Length > 0 && !part.EndsWith('.') && !part.EndsWith(' ') && part.IndexOfAny(Path.GetInvalidFileNameChars()) < 0);
        return safe ? Path.Combine([root, .. parts]) : null;
    }
}
