using System.Runtime.InteropServices;

namespace Osprey.Host;

/// <summary>
/// The package and the sandbox the host runs in [WINUI-HOST-SANDBOX]. Every application ships as its
/// own MSIX package, run in an AppContainer: it reads its own package, writes its own LocalState
/// and reaches only what its manifest's capabilities grant. Run from a build folder, the host is an
/// ordinary unpackaged process.
/// </summary>
public static partial class Sandbox
{
    const int NoPackage = 15700; // APPMODEL_ERROR_NO_PACKAGE
    const int TokenIsAppContainer = 29;
    const uint TokenQuery = 0x0008;

    /// <summary>Whether this process has a package identity.</summary>
    public static bool Packaged()
    {
        uint length = 0;
        return GetCurrentPackageFullName(ref length, 0) != NoPackage;
    }

    /// <summary>Whether this process runs in an AppContainer.</summary>
    public static bool InAppContainer()
    {
        if (!OpenProcessToken(GetCurrentProcess(), TokenQuery, out var token))
        {
            return false;
        }
        var contained = GetTokenInformation(token, TokenIsAppContainer, out var answer, sizeof(uint), out _) && answer != 0;
        CloseHandle(token);
        return contained;
    }

    /// <summary>The application's name: its package's display name, or its DLL's file name unpackaged.</summary>
    public static string Name(string library) =>
        Packaged() ? Windows.ApplicationModel.Package.Current.DisplayName : Path.GetFileNameWithoutExtension(library);

    /// <summary>One line naming where the host runs, for the self-test's trail.</summary>
    public static string Describe() =>
        (Packaged(), InAppContainer()) switch
        {
            (true, true) => "sandbox: packaged, AppContainer",
            (true, false) => "sandbox: packaged, full trust",
            _ => "sandbox: none, unpackaged",
        };

    [LibraryImport("kernel32.dll")]
    private static partial int GetCurrentPackageFullName(ref uint length, nint name);

    [LibraryImport("kernel32.dll")]
    private static partial nint GetCurrentProcess();

    [LibraryImport("advapi32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool OpenProcessToken(nint process, uint access, out nint token);

    [LibraryImport("advapi32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool GetTokenInformation(nint token, int kind, out uint answer, int length, out int returned);

    [LibraryImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool CloseHandle(nint handle);
}
