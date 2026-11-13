namespace Osprey.Host;

/// <summary>
/// This host's column of the coverage matrix [NATIVE-HOSTS-COVERAGE]: every row kind the framework
/// writes is drawn here, and every capability kind it requests is answered or refused by name. The
/// tests derive the capability names from the framework's own sources and fail when one is missing
/// from this declaration.
/// </summary>
public static class Coverage
{
    /// <summary>Every row kind this host reads and draws, in the order view.ospml numbers them.</summary>
    public static IReadOnlyList<RowKind> RowKinds { get; } = Enum.GetValues<RowKind>();

    /// <summary>Capability kinds this host answers: the adapters', and the frame clock, which the page keeps.</summary>
    public static IReadOnlyList<string> Answered { get; } = [.. Capabilities.Answered, "animate"];

    /// <summary>Capability kinds this host refuses with an error completion, not yet answered (plan 0037).</summary>
    public static IReadOnlyList<string> Unanswered { get; } =
    [
        "list-directories", "read-plists", "read-texts", "fingerprint", "http", "host-call",
        "running-applications", "open-settings", "recognise",
        "process-start", "process-write", "process-read", "process-stop",
    ];
}
