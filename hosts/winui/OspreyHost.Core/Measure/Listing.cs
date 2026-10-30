using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace Osprey.Host.Measure;

/// <summary>One child as its folder's listing reports it, without opening the child [UI-EFFECTS-MEASURE].</summary>
internal readonly record struct Child(string Name, uint Attributes, long Allocated, uint Tag, UInt128 Id)
{
    const uint HiddenAttribute = 0x0002;
    const uint DirectoryAttribute = 0x0010;
    const uint ReparseAttribute = 0x0400;
    const uint OfflineAttribute = 0x1000;
    const uint RecallOnOpenAttribute = 0x0004_0000;
    const uint RecallOnDataAccessAttribute = 0x0040_0000;
    const uint SymlinkTag = 0xA000_000C;
    const uint MountPointTag = 0xA000_0003;
    const uint AppAliasTag = 0x8000_001B;
    const uint CloudTag = 0x9000_001A;
    const uint CloudTagMask = 0xFFFF_0FFF;

    /// <summary>Hidden as File Explorer hides it: the hidden attribute alone, never a leading dot.</summary>
    public bool Hidden => (Attributes & HiddenAttribute) != 0;

    /// <summary>A folder, or a link to one.</summary>
    public bool Directory => (Attributes & DirectoryAttribute) != 0;

    /// <summary>A symbolic link or a mount point, which junctions use: reported, never followed.</summary>
    public bool Link => Reparse && Tag is SymlinkTag or MountPointTag;

    /// <summary>An app execution alias, which counts as a file of no bytes.</summary>
    public bool AppAlias => Reparse && Tag == AppAliasTag;

    /// <summary>A cloud placeholder or an offline file, which the walk never opens, so nothing downloads.</summary>
    public bool Placeholder =>
        (Reparse && (Tag & CloudTagMask) == CloudTag)
        || (Attributes & (OfflineAttribute | RecallOnOpenAttribute | RecallOnDataAccessAttribute)) != 0;

    bool Reparse => (Attributes & ReparseAttribute) != 0;
}

/// <summary>
/// Folder listings: one handle per folder and one batch of entries per call, so no child is opened
/// to learn its size, attributes, reparse tag or file id. File systems without 128-bit ids, such as
/// FAT, answer the 64-bit listing instead.
/// </summary>
internal static unsafe class Listing
{
    const int BufferSize = 64 * 1024;
    const int FileIdInfoSize = 24;

    /// <summary>Opens a folder for listing without following a link at its end.</summary>
    public static SafeFileHandle Open(string path) =>
        Native.CreateFile(Paths.Long(path), Native.ListDirectory, Native.ShareAll, 0, Native.OpenExisting, Native.BackupSemantics | Native.OpenReparsePoint, 0);

    /// <summary>A new buffer for listing folders, reused folder after folder.</summary>
    public static byte[] Buffer() => new byte[BufferSize];

    /// <summary>The serial number of the volume an open folder is on, or 0 when the file system has none.</summary>
    public static ulong Serial(SafeFileHandle folder)
    {
        var info = stackalloc byte[FileIdInfoSize];
        return Native.GetFileInformationByHandleEx(folder, Native.FileIdInfo, info, FileIdInfoSize) ? *(ulong*)info : 0;
    }

    /// <summary>Every child of an open folder in the order its file system lists them, or the error that stopped the listing.</summary>
    public static (List<Child>? Children, int Error) Children(SafeFileHandle folder, byte[] buffer)
    {
        var (children, error) = Read(folder, buffer, extended: true);
        return error is Native.ErrorInvalidParameter or Native.ErrorInvalidFunction or Native.ErrorNotSupported && children is { Count: 0 }
            ? Read(folder, buffer, extended: false)
            : (children, error);
    }

    static (List<Child>? Children, int Error) Read(SafeFileHandle folder, byte[] buffer, bool extended)
    {
        var children = new List<Child>();
        var infoClass = extended ? Native.FileIdExtdDirectoryRestartInfo : Native.FileIdBothDirectoryRestartInfo;
        fixed (byte* start = buffer)
        {
            while (Native.GetFileInformationByHandleEx(folder, infoClass, start, BufferSize))
            {
                Collect(start, children, extended);
                infoClass = extended ? Native.FileIdExtdDirectoryInfo : Native.FileIdBothDirectoryInfo;
            }
        }
        var error = Marshal.GetLastPInvokeError();
        return error == Native.ErrorNoMoreFiles ? (children, 0) : (children, error);
    }

    static void Collect(byte* entry, List<Child> children, bool extended)
    {
        while (true)
        {
            var child = extended ? Extended(entry) : Both(entry);
            if (child.Name is not "." and not "..")
            {
                children.Add(child);
            }
            var next = *(uint*)entry;
            if (next == 0)
            {
                return;
            }
            entry += next;
        }
    }

    // FILE_ID_EXTD_DIR_INFO: the 128-bit file id at 72 and the name at 88.
    static Child Extended(byte* entry) =>
        new(Name(entry, 88), *(uint*)(entry + 56), *(long*)(entry + 48), *(uint*)(entry + 68), new UInt128(*(ulong*)(entry + 80), *(ulong*)(entry + 72)));

    // FILE_ID_BOTH_DIR_INFO: the reparse tag rides in EaSize, the 64-bit file id is at 96 and the name at 104.
    static Child Both(byte* entry) =>
        new(Name(entry, 104), *(uint*)(entry + 56), *(long*)(entry + 48), *(uint*)(entry + 64), *(ulong*)(entry + 96));

    static string Name(byte* entry, int offset) => new((char*)(entry + offset), 0, (int)(*(uint*)(entry + 60) / sizeof(char)));
}

/// <summary>Path spelling for the walk: the names it reports, and the long form it opens them by.</summary>
internal static class Paths
{
    const string Verbatim = @"\\?\";
    const string Share = @"\\";

    /// <summary><paramref name="name"/> inside <paramref name="folder"/>, spelled as the person spelled the folder.</summary>
    public static string Join(string folder, string name) =>
        folder.EndsWith('\\') ? folder + name : folder + "\\" + name;

    /// <summary>The verbatim form of a path, so a path past 260 characters still opens.</summary>
    public static string Long(string path) => path switch
    {
        _ when path.StartsWith(Verbatim, StringComparison.Ordinal) => path,
        _ when path.StartsWith(Share, StringComparison.Ordinal) => Verbatim + "UNC\\" + path[Share.Length..],
        _ => Verbatim + Path.GetFullPath(path),
    };
}
