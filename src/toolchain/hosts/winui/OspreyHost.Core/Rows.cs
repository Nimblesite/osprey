using System.Text;
using Osprey.Host.Scenes;

namespace Osprey.Host;

/// <summary>The kinds of control a row can name [NATIVE-SHM-VIEW], numbered as view.ospml numbers them: a column is 0 and a sheet is 16; the told rows of [NATIVE-SHM-NODES] follow at 33.</summary>
public enum RowKind
{
    /// <summary>Children top to bottom.</summary>
    Column,

    /// <summary>Children side by side.</summary>
    Line,

    /// <summary>Text to read.</summary>
    Label,

    /// <summary>A button.</summary>
    Press,

    /// <summary>A single-line text field.</summary>
    Entry,

    /// <summary>Flexible empty space.</summary>
    Gap,

    /// <summary>A column that scrolls vertically.</summary>
    Scroll,

    /// <summary>Children on top of each other, the last in front.</summary>
    Layer,

    /// <summary>A switch that delivers "true" or "false".</summary>
    Toggle,

    /// <summary>A menu of its option children.</summary>
    Picker,

    /// <summary>An option of a picker: its text is the label and its hint the key.</summary>
    Option,

    /// <summary>A hairline, <c>Size</c> points thick.</summary>
    Rule,

    /// <summary>The image its text names.</summary>
    Image,

    /// <summary>A canvas, whose scene rows name it.</summary>
    Canvas,

    /// <summary>A text field of many lines.</summary>
    Editor,

    /// <summary>Its first child shown, the rest its context-menu items.</summary>
    Menu,

    /// <summary>Children floating above the view as a modal sheet.</summary>
    Sheet,

    /// <summary>[NATIVE-SHM-NODES] One told value: its name is its hint and its type its weight.</summary>
    Field = 33,

    /// <summary>A told record: the rows that name it as their parent are its members.</summary>
    Record = 34,

    /// <summary>A told list: the rows that name it as their parent are its items, in order.</summary>
    List = 35,
}

/// <summary>The told rows' <c>weight</c> [NATIVE-SHM-NODES]: which typed value a <c>field</c> row carries.</summary>
public static class RowValue
{
    /// <summary>The value is the row's text.</summary>
    public const long Text = 0;

    /// <summary>The value is the row's ink read as a signed whole number.</summary>
    public const long Whole = 1;

    /// <summary>The value is the row's size.</summary>
    public const long Real = 2;

    /// <summary>The value is the row's ink, zero or not.</summary>
    public const long Flag = 3;

    /// <summary>The value is the row's size, negated.</summary>
    public const long NegativeReal = 4;
}

/// <summary>The <c>traits</c> flags a row's fit carries [NATIVE-SHM-VIEW]: every bit outside <see cref="All"/> is refused.</summary>
public static class RowFlags
{
    /// <summary>The control takes no input.</summary>
    public const long Disabled = 1;

    /// <summary>It is on, chosen or selected.</summary>
    public const long Chosen = 2;

    /// <summary>A field hides its text.</summary>
    public const long Secret = 4;

    /// <summary>Text is set in a fixed-width face.</summary>
    public const long FixedWidth = 8;

    /// <summary>Text is slanted.</summary>
    public const long Slanted = 16;

    /// <summary>It is centred in its row.</summary>
    public const long Centred = 32;

    /// <summary>A field delivers only when committed.</summary>
    public const long OnCommit = 64;

    /// <summary>Every flag this host knows; a row with another bit is refused whole.</summary>
    public const long All = Disabled | Chosen | Secret | FixedWidth | Slanted | Centred | OnCommit;
}

/// <summary>How one control looks, every value decided in Osprey: <c>0xRRGGBBAA</c> ink and fill, sizes in points.</summary>
public readonly record struct Look(uint Ink, uint Fill, double Size, double Padding, double Spacing, double Radius, double Weight);

/// <summary>How one control sits: fixed extents (0 = natural), its share of free space, flags and border.</summary>
public readonly record struct Fit(double Width, double Height, double Grow, long Flags, uint Border)
{
    /// <summary>Whether the control takes no input.</summary>
    public bool Disabled => (Flags & RowFlags.Disabled) != 0;

    /// <summary>Whether it is on, chosen or selected.</summary>
    public bool On => (Flags & RowFlags.Chosen) != 0;

    /// <summary>Whether a field hides its text.</summary>
    public bool Secure => (Flags & RowFlags.Secret) != 0;

    /// <summary>Whether its text is set in a fixed-width face.</summary>
    public bool Monospaced => (Flags & RowFlags.FixedWidth) != 0;

    /// <summary>Whether its text is slanted.</summary>
    public bool Slanted => (Flags & RowFlags.Slanted) != 0;

    /// <summary>Whether it is centred in its row.</summary>
    public bool Centred => (Flags & RowFlags.Centred) != 0;

    /// <summary>Whether a field delivers only when committed.</summary>
    public bool OnCommit => (Flags & RowFlags.OnCommit) != 0;
}

/// <summary>One checked control and the rows drawn inside it [NATIVE-SHM-VIEW], with the value a told field carries [NATIVE-SHM-NODES].</summary>
public sealed record Control(RowKind Kind, long? Action, string Words, string Hint, Look Look, Fit Fit, int Parent, IReadOnlyList<int> Children, long Value = 0);

/// <summary>One field of an ask: its argument position, its whole number and its text [NATIVE-SHM-ASK].</summary>
public readonly record struct AskField(long Position, long Number, string Text);

/// <summary>A capability request or a slice write: its code, its ticket (a slice write's key) and its fields.</summary>
public sealed record Ask(long Code, string Ticket, IReadOnlyList<AskField> Fields)
{
    /// <summary>The capability code of a slice write.</summary>
    public const long SliceWrite = 100;

    /// <summary>The capability code of a refused event's diagnostic.</summary>
    public const long Diagnostic = 101;

    /// <summary>Every capability name, by code from 1, as the application's <c>Requests::kind</c> spells them.</summary>
    public static IReadOnlyList<string> Kinds { get; } =
    [
        "run", "list-directories", "read-plists", "read-texts", "fingerprint", "store-read",
        "store-write", "store-list", "store-delete", "http", "host-call", "running-applications",
        "choose-file", "choose-folder", "save-file", "measure-folders", "reveal", "open-settings",
        "cancel", "recognise", "animate", "process-start", "process-write", "process-read",
        "process-stop",
    ];

    /// <summary>The capability's name, or empty for a code this host does not know.</summary>
    public string Kind() => Code >= 1 && Code <= Kinds.Count ? Kinds[(int)Code - 1] : "";

    /// <summary>Every text at argument <paramref name="field"/>, in order: a list's items or a pair's halves.</summary>
    public IReadOnlyList<string> Texts(long field) => [.. Fields.Where(item => item.Position == field).Select(item => item.Text)];

    /// <summary>The first text at argument <paramref name="field"/>, or empty.</summary>
    public string Text(long field) => Texts(field).FirstOrDefault() ?? "";

    /// <summary>The whole number at argument <paramref name="field"/>, or 0.</summary>
    public long Number(long field) => Fields.FirstOrDefault(item => item.Position == field).Number;

    /// <summary>The flag at argument <paramref name="field"/>.</summary>
    public bool Flag(long field) => Number(field) != 0;

    /// <summary>The name/value pairs at argument <paramref name="field"/> (headers or environment variables).</summary>
    public IReadOnlyList<(string Name, string Value)> Pairs(long field)
    {
        var texts = Texts(field);
        var pairs = new List<(string, string)>((texts.Count + 1) / 2);
        for (var at = 0; at < texts.Count; at += 2)
        {
            pairs.Add((texts[at], at + 1 < texts.Count ? texts[at + 1] : ""));
        }
        return pairs;
    }
}

/// <summary>
/// [NATIVE-SHM-SAFETY] A sealed frame checked and read as typed values. Nothing from the application
/// is trusted: every kind, flag and code must be known, every parent must be an earlier row, every
/// text range must lie inside the text run on UTF-8 boundaries, every float must be finite and every
/// count must stay inside the rows that follow it. A frame that fails any check is refused whole and
/// the host keeps the last good view.
/// </summary>
public sealed class Shown
{
    internal Shown(IReadOnlyList<Control> controls, IReadOnlyList<int> roots, IReadOnlyDictionary<int, Scene> scenes, IReadOnlyList<Ask> asks)
    {
        Controls = controls;
        Roots = roots;
        Scenes = scenes;
        Asks = asks;
    }

    /// <summary>Every control, by row index, in drawing order.</summary>
    public IReadOnlyList<Control> Controls { get; }

    /// <summary>The rows with no parent, in drawing order.</summary>
    public IReadOnlyList<int> Roots { get; }

    /// <summary>Each canvas's scene, by its view row.</summary>
    public IReadOnlyDictionary<int, Scene> Scenes { get; }

    /// <summary>Every ask, in the order the application wrote it.</summary>
    public IReadOnlyList<Ask> Asks { get; }

    /// <summary>The capability requests to answer, in order.</summary>
    public IEnumerable<Ask> Requests => Asks.Where(ask => ask.Code is >= 1 and <= 25);

    /// <summary>The slice writes, as key and text.</summary>
    public IEnumerable<(string Key, string Text)> Slices =>
        Asks.Where(ask => ask.Code == Ask.SliceWrite)
            .Select(ask => (ask.Ticket, ask.Fields.Count > 0 ? ask.Fields[0].Text : ""));

    /// <summary>Why the application refused the event this frame answers, if it did.</summary>
    public IEnumerable<string> Diagnostics => Asks.Where(ask => ask.Code == Ask.Diagnostic).Select(ask => ask.Ticket);
}

/// <summary>Checks and reads a whole frame's four runs, exactly as the reference host's rows reader does.</summary>
public static class FrameReader
{
    // The row kinds view.ospml numbers: a column is 0 and a sheet is 16; told.ospml numbers the
    // three told kinds that follow at 33.
    const int KindCount = 17;

    /// <summary>Whether a row tells a component rather than draws a control [NATIVE-SHM-NODES].</summary>
    public static bool Tells(RowKind kind) => kind is RowKind.Field or RowKind.Record or RowKind.List;

    // Invalid bytes refuse the frame: the application wrote text that is not UTF-8.
    static readonly UTF8Encoding Strict = new(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);

    /// <summary>Checks and reads the frame's runs; a frame that fails any check is refused whole.</summary>
    public static Shown Read(Frame frame) =>
        Read(frame.Shapes.Rows, frame.Views.Rows, frame.Text.Bytes, frame.Asks.Rows);

    /// <summary>Checks and reads four runs of rows.</summary>
    public static Shown Read(ReadOnlySpan<NativeShape> shapes, ReadOnlySpan<NativeView> views, ReadOnlySpan<byte> text, ReadOnlySpan<NativeAsk> asks)
    {
        var (controls, roots) = Controls(views, text);
        var canvases = new List<int>();
        for (var index = 0; index < controls.Count; index++)
        {
            // A scene is a canvas row, or the told record its scene rows name [NATIVE-SHM-NODES].
            if (controls[index].Kind is RowKind.Canvas or RowKind.Record)
            {
                canvases.Add(index);
            }
        }
        return new Shown(controls, roots, SceneReader.Read(shapes, text, canvases), Asked(asks, text));
    }

    static (List<Control>, List<int>) Controls(ReadOnlySpan<NativeView> rows, ReadOnlySpan<byte> run)
    {
        var all = new List<Control>(rows.Length);
        var seated = new List<List<int>>(rows.Length);
        var roots = new List<int>();
        for (var index = 0; index < rows.Length; index++)
        {
            var row = rows[index];
            var children = new List<int>();
            all.Add(Control(row, index, run, children));
            seated.Add(children);
            if (row.Parent == -1)
            {
                roots.Add(index);
            }
            else if (row.Parent < 0 || row.Parent >= index)
            {
                throw new HostException($"Row {index} names a parent that is not an earlier row");
            }
            else
            {
                seated[(int)row.Parent].Add(index);
            }
        }
        return (all, roots);
    }

    static Control Control(NativeView row, int index, ReadOnlySpan<byte> run, List<int> children)
    {
        if (row.Kind < 0 || (row.Kind >= KindCount && row.Kind is not ((long)RowKind.Field or (long)RowKind.Record or (long)RowKind.List)))
        {
            throw new HostException($"Row {index} has an unknown kind {row.Kind}");
        }
        if (row.Kind >= KindCount)
        {
            return Told((RowKind)row.Kind, row, index, run, children);
        }
        if ((row.Flags & ~RowFlags.All) != 0)
        {
            throw new HostException($"Row {index} has unknown flags {row.Flags}");
        }
        if (row.Weight is < 100 or > 900)
        {
            throw new HostException($"Row {index} has a font weight outside 100 to 900");
        }
        return new Control(
            (RowKind)row.Kind,
            row.Action >= 0 ? row.Action : null,
            Text(run, row.TextOffset, row.TextLength, index),
            Text(run, row.HintOffset, row.HintLength, index),
            new Look(Colour(row.Colour, index), Colour(row.Background, index), Extent(row.Size, index), Extent(row.Padding, index),
                Extent(row.Spacing, index), Extent(row.Radius, index), row.Weight),
            new Fit(Extent(row.Width, index), Extent(row.Height, index), Extent(row.Grow, index), row.Flags, Colour(row.Border, index)),
            row.Parent == -1 ? -1 : (int)row.Parent,
            children);
    }

    /// <summary>
    /// One told row [NATIVE-SHM-NODES]: its name is its hint, a field's value is its words, its whole
    /// number or its real, and its weight says which (0 to 4). Its ink is a value, not a colour, so the
    /// usual checks for a drawn row do not apply; a weight outside 0 to 4 refuses the frame.
    /// </summary>
    static Control Told(RowKind kind, NativeView row, int index, ReadOnlySpan<byte> run, List<int> children)
    {
        if (row.Weight is < RowValue.Text or > RowValue.NegativeReal)
        {
            throw new HostException($"Row {index} has a told value of an unknown type {row.Weight}");
        }
        return new Control(kind, null, Text(run, row.TextOffset, row.TextLength, index), Text(run, row.HintOffset, row.HintLength, index),
            new Look(0, 0, Extent(row.Size, index), 0, 0, 0, row.Weight), new Fit(0, 0, 0, 0, 0),
            row.Parent == -1 ? -1 : (int)row.Parent, children, row.Colour);
    }

    static List<Ask> Asked(ReadOnlySpan<NativeAsk> rows, ReadOnlySpan<byte> run)
    {
        var pending = new List<(long Code, string Ticket, List<AskField> Fields)>();
        for (var index = 0; index < rows.Length; index++)
        {
            var row = rows[index];
            var field = new AskField(row.Field, row.Number, Text(run, row.TextOffset, row.TextLength, index));
            if (row.Field == 0)
            {
                pending.Add((row.Number, field.Text, []));
            }
            else if (row.Field > 0 && pending.Count > 0)
            {
                pending[^1].Fields.Add(field);
            }
            else
            {
                throw new HostException("Ask row before any ask, or with a field below zero");
            }
        }
        var all = pending.Select(ask => new Ask(ask.Code, ask.Ticket, ask.Fields)).ToList();
        if (all.Any(ask => ask.Code is < 1 or > 25 and not Ask.SliceWrite and not Ask.Diagnostic))
        {
            throw new HostException("An ask with an unknown capability code");
        }
        return all;
    }

    /// <summary>Text bytes <paramref name="offset"/>..<paramref name="offset"/> + <paramref name="length"/> of the run, checked as UTF-8.</summary>
    internal static string Text(ReadOnlySpan<byte> run, long offset, long length, int row)
    {
        if (offset < 0)
        {
            throw new HostException($"Row {row} has a text offset below zero");
        }
        if (length < 0)
        {
            throw new HostException($"Row {row} has a text length below zero");
        }
        if (offset > run.Length || length > run.Length - offset)
        {
            throw new HostException($"Row {row} names a text range outside the frame");
        }
        try
        {
            return Strict.GetString(run.Slice((int)offset, (int)length));
        }
        catch (DecoderFallbackException)
        {
            throw new HostException($"Row {row} names text that is not UTF-8");
        }
    }

    /// <summary>A number a host can lay out: finite and zero or positive, or the frame is refused.</summary>
    internal static double Extent(double value, int row) =>
        double.IsFinite(value) && value >= 0
            ? value
            : throw new HostException($"Row {row} has an extent that is negative or not finite");

    /// <summary>A packed <c>0xRRGGBBAA</c> colour, or the frame is refused.</summary>
    internal static uint Colour(long value, int row) =>
        value is >= 0 and <= uint.MaxValue
            ? (uint)value
            : throw new HostException($"Row {row} has a colour outside 0xRRGGBBAA");

    /// <summary>Refuses the frame when any of a scene row's five floating values is not finite.</summary>
    internal static void Finite(NativeShape row, int at)
    {
        if (!double.IsFinite(row.X) || !double.IsFinite(row.Y) || !double.IsFinite(row.Width) || !double.IsFinite(row.Height) || !double.IsFinite(row.Angle))
        {
            throw new HostException($"Scene row {at} has a number that is not finite");
        }
    }
}
