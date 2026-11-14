namespace Osprey.Host;

/// <summary>What a told value is [NATIVE-SHM-NODES]: nothing, a flag, a number, text, the items of a list or the members of a record.</summary>
public enum NodeKind
{
    /// <summary>No value at all: an absent member, or a frame that told none.</summary>
    Nothing,

    /// <summary>On or off.</summary>
    Flag,

    /// <summary>A whole number or a real.</summary>
    Number,

    /// <summary>Words.</summary>
    Text,

    /// <summary>The items of a list, in order.</summary>
    List,

    /// <summary>The members of a record, by name.</summary>
    Record,
}

/// <summary>
/// One value an application told [NATIVE-SHM-NODES], read from the checked rows of the shared frame:
/// a <c>field</c> row is one typed value, a <c>record</c> row a component or one of its members and a
/// <c>list</c> row the items under one name. Nothing here is parsed from text: every value is a row
/// the frame check already accepted.
/// </summary>
public readonly struct Node : IEquatable<Node>
{
    static readonly IReadOnlyList<Node> NoItems = [];
    static readonly IReadOnlyDictionary<string, Node> NoMembers = new Dictionary<string, Node>();

    readonly object? value;

    Node(NodeKind kind, object? value)
    {
        Kind = kind;
        this.value = value;
    }

    /// <summary>What the value is; an absent member and an empty frame read as <see cref="NodeKind.Nothing"/>.</summary>
    public NodeKind Kind { get; }

    /// <summary>The words of a text value, or empty.</summary>
    public string Words => value as string ?? "";

    /// <summary>Whether a flag value is on.</summary>
    public bool On => value is true;

    /// <summary>The whole number a number value holds, or 0.</summary>
    public long Whole => value switch
    {
        long whole => whole,
        double real when double.IsFinite(real) && Math.Abs(real) < 9e15 => (long)real,
        _ => 0,
    };

    /// <summary>The real number a number value holds, or 0.</summary>
    public double Decimal => value switch
    {
        long whole => whole,
        double real => real,
        _ => 0,
    };

    /// <summary>The items of a list value, or none.</summary>
    public IReadOnlyList<Node> List => value as IReadOnlyList<Node> ?? NoItems;

    /// <summary>The members of a record value, or none.</summary>
    public IReadOnlyDictionary<string, Node> Record => value as IReadOnlyDictionary<string, Node> ?? NoMembers;

    /// <summary>Whether the value is a record, so it can hold members.</summary>
    public bool IsRecord => Kind == NodeKind.Record;

    /// <summary>Whether the value is a list.</summary>
    public bool IsList => Kind == NodeKind.List;

    /// <summary>Words as a text value.</summary>
    public static Node Of(string words) => new(NodeKind.Text, words);

    /// <summary>A state as a flag value.</summary>
    public static Node Of(bool on) => new(NodeKind.Flag, on);

    /// <summary>A whole number as a number value.</summary>
    public static Node Of(long whole) => new(NodeKind.Number, whole);

    /// <summary>A real as a number value.</summary>
    public static Node Of(double real) => new(NodeKind.Number, real);

    /// <summary>Items as a list value.</summary>
    public static Node Of(IReadOnlyList<Node> items) => new(NodeKind.List, items);

    /// <summary>Members as a record value.</summary>
    public static Node Of(IReadOnlyDictionary<string, Node> members) => new(NodeKind.Record, members);

    /// <summary>The member called <paramref name="name"/>, or nothing.</summary>
    public Node Member(string name) =>
        value is IReadOnlyDictionary<string, Node> members && members.TryGetValue(name, out var found) ? found : default;

    /// <summary>Whether the record holds a member called <paramref name="name"/>.</summary>
    public bool Has(string name) => value is IReadOnlyDictionary<string, Node> members && members.ContainsKey(name);

    /// <summary>A text member; absent reads as empty.</summary>
    public string Text(string name) => Member(name).Words;

    /// <summary>A whole-number member; absent reads as 0.</summary>
    public long Number(string name) => Member(name).Whole;

    /// <summary>A real member; absent reads as 0.</summary>
    public double Real(string name) => Member(name).Decimal;

    /// <summary>A flag member; absent reads as false.</summary>
    public bool Flag(string name) => Member(name).On;

    /// <summary>A list member's items; absent reads as none.</summary>
    public IReadOnlyList<Node> Items(string name) => Member(name).List;

    /// <summary>A list member's items, mapped; absent reads as none.</summary>
    public IReadOnlyList<T> Items<T>(string name, Func<Node, T> map) => [.. Items(name).Select(map)];

    /// <summary>Every text a list member holds, in order.</summary>
    public IReadOnlyList<string> Texts(string name) => Items(name, item => item.Words);

    /// <summary>Whether two told values are the same value, member by member and item by item.</summary>
    public bool Equals(Node other)
    {
        // A frame may nest as deep as its rows do, so the walk carries its own stack rather than the host's.
        var pending = new Stack<(Node First, Node Second)>();
        pending.Push((this, other));
        while (pending.TryPop(out var pair))
        {
            var (first, second) = pair;
            if (first.Kind != second.Kind)
            {
                return false;
            }
            switch (first.Kind)
            {
                case NodeKind.Nothing:
                    break;
                case NodeKind.List:
                    if (first.List.Count != second.List.Count)
                    {
                        return false;
                    }
                    for (var at = 0; at < first.List.Count; at++)
                    {
                        pending.Push((first.List[at], second.List[at]));
                    }
                    break;
                case NodeKind.Record:
                    if (first.Record.Count != second.Record.Count)
                    {
                        return false;
                    }
                    foreach (var member in first.Record)
                    {
                        if (!second.Record.TryGetValue(member.Key, out var found))
                        {
                            return false;
                        }
                        pending.Push((member.Value, found));
                    }
                    break;
                default:
                    if (!(first.value?.Equals(second.value) ?? second.value is null))
                    {
                        return false;
                    }
                    break;
            }
        }
        return true;
    }

    /// <inheritdoc/>
    public override bool Equals(object? other) => other is Node node && Equals(node);

    /// <inheritdoc/>
    public override int GetHashCode() => HashCode.Combine(Kind, Kind switch
    {
        NodeKind.Text => Words.GetHashCode(StringComparison.Ordinal),
        NodeKind.Flag => On.GetHashCode(),
        NodeKind.Number => Whole.GetHashCode() ^ Decimal.GetHashCode(),
        _ => 0,
    });

    /// <summary>Whether two told values are the same.</summary>
    public static bool operator ==(Node first, Node second) => first.Equals(second);

    /// <summary>Whether two told values differ.</summary>
    public static bool operator !=(Node first, Node second) => !first.Equals(second);
}

/// <summary>One colour role, <c>#RRGGBB</c> in each appearance [UI-THEME].</summary>
public sealed record ThemeToken(string Name, string Light, string Dark);

/// <summary>The application's theme: colour tokens, a spacing scale and motion durations [UI-THEME].</summary>
public sealed record Theme(IReadOnlyList<ThemeToken> Tokens, IReadOnlyList<double> Spacing, long Feedback, long Settle)
{
    /// <summary>The theme a told record names; null when it is no record.</summary>
    public static Theme? Of(Node told) =>
        told.IsRecord
            ? new Theme(told.Items("tokens", token => new ThemeToken(token.Text("name"), token.Text("light"), token.Text("dark"))),
                told.Items("spacing", step => step.Decimal), told.Number("feedback"), told.Number("settle"))
            : null;
}

/// <summary>The frame a host draws [NATIVE-SHM-NODES]: the theme and every region's component tree, by key.</summary>
public sealed record ToldFrame(Theme? Theme, IReadOnlyList<(string Key, Node Node)> Regions);

/// <summary>
/// Reads the told rows of a checked frame into trees [NATIVE-SHM-NODES]. A record that a scene names
/// carries its row as <c>sceneRow</c>, so the canvas finds the scene the frame gave it; a region whose
/// frame asks for a viewport this host does not draw is refused whole.
/// </summary>
public static class Told
{
    /// <summary>The name of the record every frame opens with: the application's theme.</summary>
    public const string ThemeKey = "theme";

    /// <summary>The name of each record holding one region, by the key a <c>slot</c> names it.</summary>
    public const string RegionKey = "region";

    /// <summary>The root region every window draws first.</summary>
    public const string Root = "root";

    /// <summary>
    /// The deepest tree this host reads. A frame can nest as deep as it has rows, so the bound main's
    /// JSON decoder enforced is kept: a tree past it is refused rather than drawn into the host's stack.
    /// </summary>
    public const int Depth = 256;

    /// <summary>The frame's theme and regions, or null when the frame tells no components: a frame of plain view rows, which the row view draws.</summary>
    public static ToldFrame? Frame(Shown shown)
    {
        var told = shown.Roots.Where(root => shown.Controls[root].Kind == RowKind.Record).ToList();
        if (told.Count == 0)
        {
            return null;
        }
        var values = Trees(shown);
        Theme? theme = null;
        var regions = new List<(string Key, Node Node)>(told.Count);
        foreach (var root in told)
        {
            var value = values[root];
            switch (shown.Controls[root].Hint)
            {
                case ThemeKey:
                    theme = Theme.Of(value);
                    break;
                case RegionKey:
                    var key = value.Text("key");
                    var node = value.Member("node");
                    Viewports.Refuse(key, node);
                    regions.Add((key, node));
                    break;
                default:
                    break;
            }
        }
        return new ToldFrame(theme, regions);
    }

    /// <summary>
    /// Every row's value. A parent is always an earlier row, so building from the last row back means
    /// a row's children are built before it and no depth of tree costs the host its stack.
    /// </summary>
    static Node[] Trees(Shown shown)
    {
        var built = new Node[shown.Controls.Count];
        var depths = new int[shown.Controls.Count];
        for (var index = 0; index < shown.Controls.Count; index++)
        {
            var parent = shown.Controls[index].Parent;
            depths[index] = parent < 0 ? 0 : depths[parent] + 1;
            if (depths[index] > Depth)
            {
                throw new HostException($"Row {index} nests deeper than the {Depth} levels this host draws");
            }
        }
        for (var index = shown.Controls.Count - 1; index >= 0; index--)
        {
            var row = shown.Controls[index];
            built[index] = row.Kind switch
            {
                RowKind.Field => Value(row),
                RowKind.List => Node.Of([.. row.Children.Select(child => built[child])]),
                _ => ToldRecord(shown, row, index, built),
            };
        }
        return built;
    }

    /// <summary>One field row's typed value [NATIVE-SHM-NODES]: its weight says which of the four it carries.</summary>
    static Node Value(Control row) => row.Look.Weight switch
    {
        RowValue.Whole => Node.Of(row.Value),
        RowValue.Real => Node.Of(row.Look.Size),
        RowValue.Flag => Node.Of(row.Value != 0),
        RowValue.NegativeReal => Node.Of(-row.Look.Size),
        _ => Node.Of(row.Words),
    };

    // One record row's members, by name; an unnamed member is not a member at all.
    static Node ToldRecord(Shown shown, Control row, int index, Node[] built)
    {
        var members = new Dictionary<string, Node>(StringComparer.Ordinal);
        foreach (var child in row.Children.Where(child => shown.Controls[child].Hint.Length > 0))
        {
            members[shown.Controls[child].Hint] = built[child];
        }
        if (shown.Scenes.ContainsKey(index))
        {
            members["sceneRow"] = Node.Of((long)index);
        }
        return Node.Of(members);
    }
}
