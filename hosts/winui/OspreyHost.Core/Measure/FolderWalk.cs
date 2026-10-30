using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;

namespace Osprey.Host.Measure;

/// <summary>
/// The <c>measure-folders</c> capability [UI-EFFECTS-MEASURE]: each folder's children measured to
/// the bottom within an entry limit and a deadline, written as the slice Osprey named. It sorts,
/// hides, formats and decides nothing; Osprey does all of that.
/// </summary>
public static class FolderWalk
{
    /// <summary>
    /// [UI-EFFECTS-MEASURE] Measures each folder's children to the bottom and returns the slice text
    /// <c>{"folders":[...]}</c> in the order the folders were given. The deadline runs from this call
    /// across all of them; once it passes, nothing more is entered and what was measured is marked
    /// truncated. Throws <see cref="OperationCanceledException"/> when <paramref name="cancel"/> fires.
    /// </summary>
    public static string Measure(IReadOnlyList<string> folders, int entryLimit, TimeSpan deadline, CancellationToken cancel)
    {
        cancel.ThrowIfCancellationRequested();
        var budget = new Budget(deadline, cancel);
        var seen = new HashSet<FileKey>();
        var measured = new List<Measured>(folders.Count);
        foreach (var folder in folders)
        {
            measured.Add(One(folder, Math.Max(0, entryLimit), budget, seen));
        }
        return Report.Write(measured);
    }

    static Measured One(string path, int limit, Budget budget, HashSet<FileKey> seen)
    {
        var volume = Volumes.Of(path);
        if (budget.Expired)
        {
            return Opened(path, volume, "", missing: false, truncated: true);
        }
        using var folder = Listing.Open(path);
        if (folder.IsInvalid)
        {
            var error = Marshal.GetLastPInvokeError();
            return error is Native.ErrorFileNotFound or Native.ErrorPathNotFound
                ? Opened(path, volume, "", missing: true, truncated: false)
                : Opened(path, volume, new Win32Exception(error).Message, missing: false, truncated: false);
        }
        return Listing.Children(folder, Listing.Buffer()) switch
        {
            ({ } children, 0) => new Walker(Listing.Serial(folder), budget, seen).Folder(path, children, limit, volume),
            (_, var error) => Opened(path, volume, new Win32Exception(error).Message, missing: false, truncated: false),
        };
    }

    static Measured Opened(string path, Volume volume, string error, bool missing, bool truncated) =>
        new(path, error, missing, truncated, 0, 0, 0, 0, volume, []);
}

/// <summary>The volume a folder is on, from the volume's own answers; a call that fails leaves its part empty.</summary>
internal static unsafe class Volumes
{
    /// <summary>The label, capacity and space available to this user of the volume holding <paramref name="path"/>.</summary>
    public static Volume Of(string path)
    {
        var root = stackalloc char[Native.RootLength];
        if (!Native.GetVolumePathName(path, root, Native.RootLength))
        {
            return new Volume("", 0, 0);
        }
        var top = new string(root);
        var (total, available) = Native.GetDiskFreeSpaceEx(top, out var free, out var size, out _) ? ((long)size, (long)free) : (0L, 0L);
        return new Volume(Label(top) is { Length: > 0 } label ? label : top, total, available);
    }

    static string Label(string root)
    {
        var label = stackalloc char[Native.RootLength];
        return Native.GetVolumeInformation(root, label, Native.RootLength, 0, 0, 0, 0, 0) ? new string(label) : "";
    }
}

/// <summary>The slice text, written with the keys in the order [UI-EFFECTS-MEASURE] documents.</summary>
internal static class Report
{
    /// <summary><c>{"folders":[...]}</c> for the measured folders, in order.</summary>
    public static string Write(IReadOnlyList<Measured> folders) => Json.Object(writer =>
    {
        writer.WriteStartArray("folders");
        foreach (var folder in folders)
        {
            Folder(writer, folder);
        }
        writer.WriteEndArray();
    });

    static void Folder(Utf8JsonWriter writer, Measured folder)
    {
        writer.WriteStartObject();
        Text(writer, "path", folder.Path);
        Text(writer, "error", folder.Error);
        writer.WriteBoolean("missing", folder.Missing);
        writer.WriteBoolean("truncated", folder.Truncated);
        writer.WriteNumber("unreadable", folder.Unreadable);
        writer.WriteNumber("bytes", folder.Bytes);
        writer.WriteNumber("visible", folder.Visible);
        writer.WriteNumber("items", folder.Items);
        writer.WriteStartObject("volume");
        Text(writer, "name", folder.Volume.Name);
        writer.WriteNumber("total", folder.Volume.Total);
        writer.WriteNumber("available", folder.Volume.Available);
        writer.WriteEndObject();
        Entries(writer, folder.Entries);
        writer.WriteEndObject();
    }

    static void Entries(Utf8JsonWriter writer, IReadOnlyList<Entry> entries)
    {
        writer.WriteStartArray("entries");
        foreach (var entry in entries)
        {
            writer.WriteStartObject();
            Text(writer, "name", entry.Name);
            Text(writer, "path", entry.Path);
            writer.WriteNumber("bytes", entry.Bytes);
            writer.WriteNumber("visible", entry.Visible);
            writer.WriteNumber("items", entry.Items);
            writer.WriteBoolean("hidden", entry.Hidden);
            writer.WriteBoolean("folder", entry.Folder);
            writer.WriteBoolean("link", entry.Link);
            writer.WriteBoolean("guarded", entry.Guarded);
            writer.WriteEndObject();
        }
        writer.WriteEndArray();
    }

    // NTFS allows names that are not valid UTF-16; their unpaired surrogates become U+FFFD rather than failing the walk.
    static void Text(Utf8JsonWriter writer, string name, string value) => writer.WriteString(name, Encoding.UTF8.GetBytes(value));
}
