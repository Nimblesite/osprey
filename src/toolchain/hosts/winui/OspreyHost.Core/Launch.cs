namespace Osprey.Host;

/// <summary>
/// The command line: <c>OspreyHost.exe [--headless] [--self-test script.json shots-folder] app.dll [arguments...]</c>.
/// The arguments after the DLL reach the application's launch context unchanged. A headless window
/// is drawn off screen and never activated, so a test runs without anything appearing or taking focus.
/// </summary>
public sealed record Launch(string Library, string[] Arguments, string? Script, string? Shots, bool Headless = false)
{
    const string SelfTestFlag = "--self-test";
    const string HeadlessFlag = "--headless";
    const string AppPlaceholder = "{app}";

    /// <summary>How to start the host, shown when the command line names no application.</summary>
    public const string Usage = "usage: OspreyHost.exe [--headless] [--self-test <script.json> <shots-folder>] <app.dll> [arguments...]";

    /// <summary>The launch the command line asks for, or null when it names no application.</summary>
    public static Launch? Parse(string[] args) => args switch
    {
        [HeadlessFlag, .. var rest] => Parse(rest) is { } launch ? launch with { Headless = true } : null,
        [SelfTestFlag, var script, var shots, var library, .. var rest] => new Launch(library, rest, script, shots),
        [SelfTestFlag, ..] => null,
        [var library, .. var rest] => new Launch(library, rest, null, null),
        _ => null,
    };

    /// <summary>
    /// The command line a packaged host runs [WINUI-HOST-SANDBOX]. Its package holds the application
    /// as <c>app\app.dll</c> beside the host and its default arguments one per line in
    /// <c>app\arguments.txt</c>, where <c>{app}</c> names that folder. Started from the Start menu,
    /// or with only a self-test, the host runs that application; any other command line is its own.
    /// </summary>
    public static string[] Packaged(string[] args, string root)
    {
        var app = Path.Combine(root, "app");
        var library = Path.Combine(app, "app.dll");
        if (!File.Exists(library))
        {
            return args;
        }
        var file = Path.Combine(app, "arguments.txt");
        string[] defaults = File.Exists(file)
            ? [.. File.ReadAllLines(file).Where(line => line.Length > 0).Select(line => line.Replace(AppPlaceholder, app))]
            : [];
        return args switch
        {
            [] => [library, .. defaults],
            [HeadlessFlag, .. var rest] => [HeadlessFlag, .. Packaged(rest, root)],
            [SelfTestFlag, var script, var shots] => [SelfTestFlag, script, shots, library, .. defaults],
            _ => args,
        };
    }
}
