namespace Osprey.Host;

/// <summary>
/// This host's column of the coverage matrix [NATIVE-HOSTS-COVERAGE]: every node kind and capability
/// kind the framework defines, as drawn, answered or rejected here. The tests derive the framework's
/// kinds from its Osprey sources and fail when one is missing from this declaration, and the
/// renderer refuses to start unless it draws exactly the kinds declared drawn.
/// </summary>
public static class Coverage
{
    /// <summary>Node kinds this host draws with WinUI controls.</summary>
    public static IReadOnlyList<string> Drawn { get; } =
    [
        "text", "stack", "section", "screen", "split", "layer", "context-menu", "aside", "tabs", "slot",
        "pair", "badge", "label", "icon", "link", "spacer", "divider", "segments", "empty", "progress",
        "meter", "swatch", "rows", "button", "field", "toggle", "picker", "disclosure", "alert", "scene",
    ];

    /// <summary>Node kinds this host refuses with a visible diagnostic: none.</summary>
    public static IReadOnlyList<string> Refused { get; } = [];

    /// <summary>Capability kinds this host answers: the adapters', and the frame clock, which the window keeps.</summary>
    public static IReadOnlyList<string> Answered { get; } = [.. Capabilities.Answered, "animate"];

    /// <summary>Capability kinds this host refuses with an error completion, not yet answered (plan 0038 phase 4).</summary>
    public static IReadOnlyList<string> Unanswered { get; } =
    [
        "run", "list-directories", "read-texts", "http", "host-call", "choose-file", "save-file",
    ];
}
