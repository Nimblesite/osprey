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
    const uint GenericRead = 0x80000000;
    const uint ListDirectory = 0x0001;
    const uint OpenExisting = 3;
    const uint BackupSemantics = 0x02000000;
    const uint OpenReparsePoint = 0x00200000;
    const int ErrorFileNotFound = 2;
    const int ErrorPathNotFound = 3;

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

    Completion Read(Ask ask) => Named(ask, (name, _) =>
    {
        var parts = name.Split(['/', '\\']);
        return WithPinned(parts.AsSpan(0, parts.Length - 1), FileShare.Read, create: false,
            folder => ReadLeaf(ask, name, Path.Combine(folder, parts[^1])))
            ?? Completion.Failed(ask.Ticket, $"no such store entry {name}");
    });

    Completion ReadLeaf(Ask ask, string name, string path)
    {
        var handle = CreateFileFromAppW(path, GenericRead, FileShare.Read, 0, OpenExisting, OpenReparsePoint, 0);
        if (handle.IsInvalid)
        {
            var error = Marshal.GetLastPInvokeError();
            handle.Dispose();
            return error is ErrorFileNotFound or ErrorPathNotFound
                ? Completion.Failed(ask.Ticket, $"no such store entry {name}")
                : throw new IOException(new Win32Exception(error).Message);
        }
        try
        {
            var attributes = File.GetAttributes(handle);
            if ((attributes & (FileAttributes.Directory | FileAttributes.ReparsePoint)) != 0)
                throw new IOException($"invalid store entry {name}");
            using var stream = new FileStream(handle, FileAccess.Read);
            var limit = ask.Number(2);
            if (stream.Length > limit)
                return Completion.Failed(ask.Ticket, $"store entry {name} exceeds limit {limit}");
            var bytes = new byte[stream.Length];
            stream.ReadExactly(bytes);
            return Completion.Done(ask.Ticket, "null", new SliceWrite(ask.Text(3), Strict.GetString(bytes)));
        }
        catch
        {
            handle.Dispose();
            throw;
        }
    }

    Completion Write(Ask ask) => Named(ask, (name, _) =>
    {
        if (Slices.Get(ask.Text(2)) is not { } text)
            return Completion.Failed(ask.Ticket, "store-write slice is missing");
        var parts = name.Split(['/', '\\']);
        WithPinned(parts.AsSpan(0, parts.Length - 1), FileShare.ReadWrite, create: true, folder =>
        {
            Replace(folder, parts[^1], Strict.GetBytes(text));
            return Completion.Done(ask.Ticket, "null");
        });
        return Completion.Done(ask.Ticket, "null");
    });

    // [WINUI-HOST-CAPABILITIES] Pin every directory from the store root to the target folder
    // without delete sharing, so no intermediate component can be a junction or swapped for one.
    Completion? WithPinned(ReadOnlySpan<string> parts, FileShare leafShare, bool create, Func<string, Completion> act)
    {
        var folder = Path.GetFullPath(root);
        var directories = new Stack<SafeFileHandle>();
        try
        {
            for (var i = 0; i < parts.Length; i++)
            {
                if (PinDirectory(folder, FileShare.Read, create) is not { } handle) return null;
                directories.Push(handle);
                folder = Path.Combine(folder, parts[i]);
            }
            if (PinDirectory(folder, leafShare, create) is not { } leaf) return null;
            directories.Push(leaf);
            return act(folder);
        }
        finally
        {
            while (directories.TryPop(out var directory)) directory.Dispose();
        }
    }

    // [WINUI-HOST-CAPABILITIES] Hold each directory without delete sharing, so it cannot be
    // renamed, removed or replaced by a junction during a store operation. Ancestors also refuse
    // write sharing; the entry's own folder shares writing when replacing or deleting a leaf.
    static SafeFileHandle? PinDirectory(string path, FileShare share, bool create)
    {
        if (create) Directory.CreateDirectory(path);
        var handle = CreateFileFromAppW(path, ListDirectory, share, 0, OpenExisting, BackupSemantics | OpenReparsePoint, 0);
        try
        {
            if (handle.IsInvalid)
            {
                var error = Marshal.GetLastPInvokeError();
                if (!create && error is ErrorFileNotFound or ErrorPathNotFound)
                {
                    handle.Dispose();
                    return null;
                }
                throw new IOException(new Win32Exception(error).Message);
            }
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

    Completion List(Ask ask) => Named(ask, (name, _) =>
    {
        var parts = name.Length == 0 ? [] : name.Split(['/', '\\']);
        return WithPinned(parts, FileShare.Read, create: false, folder =>
            Completion.Done(ask.Ticket, Docs.Array(Directory.EnumerateFileSystemEntries(folder).Select(path => Path.GetFileName(path) ?? "").Order(StringComparer.Ordinal))))
            ?? Completion.Done(ask.Ticket, "[]");
    }, folder: true);

    Completion Delete(Ask ask) => Named(ask, (name, _) =>
    {
        var parts = name.Split(['/', '\\']);
        return WithPinned(parts.AsSpan(0, parts.Length - 1), FileShare.ReadWrite, create: false, folder =>
        {
            var path = Path.Combine(folder, parts[^1]);
            if (!File.Exists(path)) return Completion.Failed(ask.Ticket, $"no such store entry {name}");
            File.Delete(path);
            return Completion.Done(ask.Ticket, "null");
        }) ?? Completion.Failed(ask.Ticket, $"no such store entry {name}");
    });

    // Answers with `act` on the requested name and its path when the name stays inside the store.
    Completion Named(Ask ask, Func<string, string, Completion> act, bool folder = false)
    {
        var name = ask.Text(1);
        return Entry(name, folder) is { } path ? act(name, path) : Completion.Failed(ask.Ticket, $"invalid store entry {name}");
    }

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
