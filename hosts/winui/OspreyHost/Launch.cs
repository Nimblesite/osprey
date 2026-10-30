namespace Osprey.Host.WinUI;

/// <summary>
/// The command line: <c>OspreyHost.exe [--self-test script.json shots-folder] app.dll [arguments...]</c>.
/// The arguments after the DLL reach the application's launch context unchanged.
/// </summary>
sealed record Launch(string Library, string[] Arguments, string? Script, string? Shots)
{
    const string SelfTestFlag = "--self-test";

    /// <summary>How to start the host, shown when the command line names no application.</summary>
    public const string Usage = "usage: OspreyHost.exe [--self-test <script.json> <shots-folder>] <app.dll> [arguments...]";

    /// <summary>The launch the command line asks for, or null when it names no application.</summary>
    public static Launch? Parse(string[] args) => args switch
    {
        [SelfTestFlag, var script, var shots, var library, .. var rest] => new Launch(library, rest, script, shots),
        [SelfTestFlag, ..] => null,
        [var library, .. var rest] => new Launch(library, rest, null, null),
        _ => null,
    };
}
