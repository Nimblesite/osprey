using System.Runtime.InteropServices;

namespace Osprey.Host;

/// <summary>File Explorer, through the shell's own API: the <c>reveal</c> capability [WINUI-HOST-CAPABILITIES].</summary>
public static partial class Explorer
{
    const int Ok = 0;

    /// <summary>Opens the folder holding <paramref name="path"/> with it selected; the error, or null.</summary>
    public static string? Reveal(string path)
    {
        if (!Path.IsPathFullyQualified(path))
        {
            return $"Reveal needs a full path, and {path} is not one";
        }
        var parsed = SHParseDisplayName(path, 0, out var item, 0, out _);
        if (parsed != Ok)
        {
            return $"File Explorer cannot find {path} (0x{parsed:X8})";
        }
        try
        {
            var opened = SHOpenFolderAndSelectItems(item, 0, 0, 0);
            return opened == Ok ? null : $"File Explorer could not show {path} (0x{opened:X8})";
        }
        finally
        {
            CoTaskMemFree(item);
        }
    }

    [LibraryImport("shell32.dll", StringMarshalling = StringMarshalling.Utf16)]
    private static partial int SHParseDisplayName(string name, nint bindContext, out nint item, uint wanted, out uint found);

    [LibraryImport("shell32.dll")]
    private static partial int SHOpenFolderAndSelectItems(nint folder, uint count, nint items, uint flags);

    [LibraryImport("ole32.dll")]
    private static partial void CoTaskMemFree(nint memory);
}
