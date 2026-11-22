namespace Osprey.Host;

/// <summary>
/// The command line: <c>OspreyHost.exe [--headless] [--self-test script.json shots-folder] [doubles] app.dll [arguments...]</c>.
/// The arguments after the DLL reach the application's launch context unchanged. A headless window
/// is drawn off screen and never activated, so a test runs without anything appearing or taking focus.
/// </summary>
public sealed record Launch(string Library, string[] Arguments, string? Script, string? Shots, bool Headless = false)
{
    const string SelfTestFlag = "--self-test";
    const string HeadlessFlag = "--headless";
    const string AppPlaceholder = "{app}";

    /// <summary>How to start the host, shown when the command line names no application.</summary>
    public const string Usage = "usage: OspreyHost.exe [--headless] [--self-test <script.json> <shots-folder>] [--store <folder>] [--http-double <replies.json>] [--folder-double <folder>] [--patience <seconds>] <app.dll> [arguments...]";

    /// <summary>What a self-test puts in place of the person's store, network and folder picker.</summary>
    public Doubles Doubles { get; init; } = Doubles.None;

    /// <summary>The launch the command line asks for, or null when it names no application.</summary>
    public static Launch? Parse(string[] args) => args switch
    {
        [HeadlessFlag, .. var rest] => Parse(rest) is { } launch ? launch with { Headless = true } : null,
        [SelfTestFlag, var script, var shots, .. var rest] => Parse(rest) is { Script: null } launch ? launch with { Script = script, Shots = shots } : null,
        [SelfTestFlag, ..] => null,
        [var flag, var value, .. var rest] when Doubles.Flags.Contains(flag) => Parse(rest) is { } launch && launch.Doubles.With(flag, value) is { } doubles ? launch with { Doubles = doubles } : null,
        [var flag, ..] when Doubles.Flags.Contains(flag) => null,
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

/// <summary>
/// [WINUI-HOST-SELF-TEST] A self-test's stand-ins: a store folder of its own, a file of model replies
/// answering <c>http</c>, a folder answering the folder picker, and how long a step may wait.
/// </summary>
public sealed record Doubles(string? Store, string? Http, string? Folder, double Patience)
{
    /// <summary>No stand-ins: the person's store, network and picker, and the default patience.</summary>
    public static Doubles None { get; } = new(null, null, null, DefaultPatience);

    /// <summary>How many seconds a step may wait by default; the WinUI harness derives its whole-journey
    /// kill from this times the script's step count.</summary>
    public const double DefaultPatience = 90;

    /// <summary>The flags that set a stand-in, each followed by its value.</summary>
    public static IReadOnlySet<string> Flags { get; } = new HashSet<string> { "--store", "--http-double", "--folder-double", "--patience" };

    /// <summary>These stand-ins with <paramref name="flag"/> set to <paramref name="value"/>; null for a patience that is not a positive number.</summary>
    public Doubles? With(string flag, string value) => flag switch
    {
        "--store" => this with { Store = value },
        "--http-double" => this with { Http = value },
        "--folder-double" => this with { Folder = value },
        _ => double.TryParse(value, System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var seconds) && seconds > 0
            ? this with { Patience = seconds }
            : null,
    };
}
