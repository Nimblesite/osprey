namespace Osprey.Host;

/// <summary>
/// This host's column of the coverage matrix [NATIVE-HOSTS-COVERAGE]: every row kind view.ospml
/// numbers and every told row told.ospml numbers is read here, every component kind the framework
/// defines is drawn or refused by name, and every capability kind is answered or refused by name.
/// The tests derive the framework's kinds from its Osprey sources and fail when one is missing from
/// this declaration, and the renderer refuses to start unless it draws exactly the kinds declared
/// drawn.
/// </summary>
public static class Coverage
{
    /// <summary>Every row kind view.ospml numbers, in its order: a column is 0 and a sheet is 16.</summary>
    public static IReadOnlyList<RowKind> RowKinds { get; } = [.. Enum.GetValues<RowKind>().Where(kind => !FrameReader.Tells(kind))];

    /// <summary>Every told row kind told.ospml numbers [NATIVE-SHM-NODES]: a field is 33, a record 34 and a list 35.</summary>
    public static IReadOnlyList<RowKind> ToldKinds { get; } = [.. Enum.GetValues<RowKind>().Where(FrameReader.Tells)];

    /// <summary>Node kinds this host draws with WinUI controls.</summary>
    public static IReadOnlyList<string> Drawn { get; } =
    [
        "text", "stack", "section", "screen", "split", "layer", "context-menu", "aside", "tabs", "slot",
        "pair", "badge", "label", "icon", "link", "spacer", "divider", "segments", "empty", "progress",
        "meter", "swatch", "rows", "button", "field", "toggle", "picker", "disclosure", "alert", "scene",
        "themed",
    ];

    /// <summary>
    /// Node kinds the framework defines without a WinUI control here, so each is replaced by the
    /// visible diagnostic <see cref="Refusal"/> and reported, never dropped or guessed.
    /// </summary>
    public static IReadOnlyList<string> Refused { get; } =
    [
        "rich-text", "framed", "dialog", "math", "image", "video", "editor", "board",
    ];

    /// <summary>Capability kinds this host answers: the adapters', and the frame clock, which the session keeps.</summary>
    public static IReadOnlyList<string> Answered { get; } = [.. Capabilities.Answered, "animate"];

    /// <summary>Capability kinds this host refuses with an error completion, not yet answered (plan 0037).</summary>
    public static IReadOnlyList<string> Unanswered { get; } =
    [
        "list-directories", "read-plists", "read-texts", "fingerprint", "http", "host-call",
        "running-applications", "open-settings", "recognise",
        "process-start", "process-write", "process-read", "process-stop",
    ];

    /// <summary>The visible diagnostic shown in place of a node kind this host does not draw.</summary>
    public static string Refusal(string kind) => $"This host does not draw {kind} nodes";
}
