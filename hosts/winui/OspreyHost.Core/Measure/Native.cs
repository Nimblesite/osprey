using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace Osprey.Host.Measure;

/// <summary>The Win32 calls the folder walk makes [UI-EFFECTS-MEASURE], source-generated so the host stays AOT-safe.</summary>
internal static unsafe partial class Native
{
    public const uint ListDirectory = 0x0001;
    public const uint ShareAll = 0x0001 | 0x0002 | 0x0004;
    public const uint OpenExisting = 3;
    public const uint BackupSemantics = 0x0200_0000;
    public const uint OpenReparsePoint = 0x0020_0000;

    public const int FileIdBothDirectoryInfo = 10;
    public const int FileIdBothDirectoryRestartInfo = 11;
    public const int FileIdInfo = 18;
    public const int FileIdExtdDirectoryInfo = 19;
    public const int FileIdExtdDirectoryRestartInfo = 20;

    public const int ErrorFileNotFound = 2;
    public const int ErrorPathNotFound = 3;
    public const int ErrorNoMoreFiles = 18;
    public const int ErrorInvalidFunction = 1;
    public const int ErrorNotSupported = 50;
    public const int ErrorInvalidParameter = 87;

    /// <summary>The longest path a volume root can have, in characters, with room for the terminator.</summary>
    public const int RootLength = 1024;

    [LibraryImport("kernel32.dll", EntryPoint = "CreateFileW", SetLastError = true, StringMarshalling = StringMarshalling.Utf16)]
    public static partial SafeFileHandle CreateFile(string name, uint access, uint share, nint security, uint disposition, uint flags, nint template);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool GetFileInformationByHandleEx(SafeFileHandle file, int infoClass, byte* buffer, uint size);

    [LibraryImport("kernel32.dll", EntryPoint = "GetVolumePathNameW", SetLastError = true, StringMarshalling = StringMarshalling.Utf16)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool GetVolumePathName(string path, char* root, uint length);

    [LibraryImport("kernel32.dll", EntryPoint = "GetDiskFreeSpaceExW", SetLastError = true, StringMarshalling = StringMarshalling.Utf16)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool GetDiskFreeSpaceEx(string root, out ulong available, out ulong total, out ulong free);

    [LibraryImport("kernel32.dll", EntryPoint = "GetVolumeInformationW", SetLastError = true, StringMarshalling = StringMarshalling.Utf16)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static partial bool GetVolumeInformation(string root, char* label, uint labelLength, nint serial, nint component, nint flags, nint system, uint systemLength);
}
