using System.Runtime.InteropServices;

namespace Osprey.Host.WinUI;

/// <summary>
/// Whether this process can show a window [WINUI-HOST-DEPLOY]. A process in a window station no one
/// can see, as a Windows service in session 0 is, cannot create a WinUI window: WinUI ends the
/// process with E_ACCESSDENIED. The host checks first and says so instead.
/// </summary>
static partial class Desktop
{
    const int FlagsIndex = 1;
    const uint Visible = 1;

    [StructLayout(LayoutKind.Sequential)]
    struct StationFlags
    {
        public int Inherit;
        public int Reserved;
        public uint Flags;
    }

    /// <summary>Why this process cannot show a window, or null when it can.</summary>
    public static string? Missing() =>
        GetUserObjectInformationW(GetProcessWindowStation(), FlagsIndex, out var station, Marshal.SizeOf<StationFlags>(), out _) && (station.Flags & Visible) == 0
            ? $"OspreyHost needs a desktop someone can see, and this process runs in an invisible window station (session {Session(Environment.ProcessId)}), as Windows services do. Start it from a signed-in desktop session."
            : null;

    static uint Session(int process) => ProcessIdToSessionId((uint)process, out var session) ? session : uint.MaxValue;

    [LibraryImport("user32.dll")]
    private static partial nint GetProcessWindowStation();

    [LibraryImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool GetUserObjectInformationW(nint item, int index, out StationFlags info, int length, out int needed);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool ProcessIdToSessionId(uint process, out uint session);
}
