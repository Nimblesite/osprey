using System.Text.RegularExpressions;

namespace Osprey.Host.Tests;

/// <summary>
/// The framework's own vocabulary read from its Osprey sources [NATIVE-HOSTS-COVERAGE], so a node,
/// capability or symbol added to @osprey/native fails these tests until this host declares it.
/// </summary>
static partial class Framework
{
    /// <summary>The repository root: the nearest folder above the tests holding the framework.</summary>
    public static string Root { get; } = Find(AppContext.BaseDirectory);

    /// <summary>The WinUI host's folder.</summary>
    public static string Host => Path.Combine(Root, "hosts", "winui");

    /// <summary>Every node kind the framework encodes, as it travels on the wire.</summary>
    public static IReadOnlyList<string> NodeKinds => [.. Variants(Source("node.ospml"), "type Node A =").Select(Wire)];

    /// <summary>Every capability kind the framework requests, as it travels on the wire.</summary>
    public static IReadOnlyList<string> CapabilityKinds => [.. Variants(Source("capability.ospml"), "type Capability =").Select(Wire)];

    /// <summary>Every symbol name the framework sends; <c>NoSymbol</c> travels as the empty string and draws nothing.</summary>
    public static IReadOnlyList<string> Symbols =>
    [
        .. Line(Source("symbol.ospml"), "type Symbol =")["type Symbol =".Length..]
            .Split('|', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries)
            .Where(name => name != "NoSymbol")
            .Select(name => name.ToLowerInvariant()),
    ];

    /// <summary>The text of one framework source file.</summary>
    public static string Source(string file) => File.ReadAllText(Path.Combine(Root, "packages", "native", "src", file));

    static string Find(string from)
    {
        for (var folder = new DirectoryInfo(from); folder is not null; folder = folder.Parent)
        {
            if (File.Exists(Path.Combine(folder.FullName, "packages", "native", "src", "node.ospml")))
            {
                return folder.FullName;
            }
        }
        throw new DirectoryNotFoundException($"No Osprey repository holds {from}");
    }

    static string Line(string source, string start) =>
        Lines(source).FirstOrDefault(line => line.StartsWith(start, StringComparison.Ordinal))
        ?? throw new InvalidDataException($"The framework declares no `{start}`");

    static IEnumerable<string> Lines(string source) => source.Split('\n').Select(line => line.TrimEnd('\r'));

    /// <summary>The variants of a union declared in block form: lines indented one level and capitalised, until the next declaration.</summary>
    static IEnumerable<string> Variants(string source, string header) =>
        Lines(source)
            .SkipWhile(line => !line.StartsWith(header, StringComparison.Ordinal))
            .Skip(1)
            .TakeWhile(line => line.Length == 0 || char.IsWhiteSpace(line[0]))
            .Where(line => line.Length > 4 && line.StartsWith("    ", StringComparison.Ordinal) && char.IsUpper(line[4]))
            .Select(line => line.Trim());

    /// <summary>A constructor's wire name: <c>ContextMenu</c> travels as <c>context-menu</c>.</summary>
    public static string Kebab(string name) => Capital().Replace(name, "-$1").ToLowerInvariant();

    // The encoder's aliases are shared by node and capability inventories.
    static string Wire(string name) => name switch
    {
        "Typeset" => "math",
        "ReadPropertyLists" => "read-plists",
        _ => Kebab(name),
    };

    [GeneratedRegex("(?<!^)([A-Z])")]
    private static partial Regex Capital();
}
