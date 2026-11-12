using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;

namespace Osprey.Host;

/// <summary>The kinds of control a row can name [NATIVE-SHM-VIEW], numbered as view.ospml numbers them: a column is 0 and a sheet is 16.</summary>
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

    /// <summary>A reserved blank that draws nothing yet.</summary>
    Canvas,

    /// <summary>A text field of many lines.</summary>
    Editor,

    /// <summary>Its first child shown, the rest its context-menu items.</summary>
    Menu,

    /// <summary>Children floating above the view as a modal sheet.</summary>
    Sheet,
}

/// <summary>
/// One control row with every value a host may safely use [NATIVE-SHM-SAFETY]: a parent that is
/// neither -1 nor an earlier row reads as none, text and hint outside the text run read as empty,
/// a number that is not finite or is negative reads as zero, and the font weight is clamped into
/// its 100..900 range. Only <see cref="Rows"/> builds one, so no reader ever trusts a raw row.
/// </summary>
public readonly struct SafeRow
{
    internal SafeRow(int index, in NativeView row, string text, string hint)
    {
        Kind = (RowKind)row.Kind;
        Parent = row.Parent == -1 || row.Parent >= 0 && row.Parent < index ? (int)row.Parent : Rows.None;
        Action = row.Action;
        (Text, Hint) = (text, hint);
        (Colour, Background) = (row.Colour, row.Background);
        (Size, Padding) = (Clean(row.Size), Clean(row.Padding));
        (Spacing, Radius) = (Clean(row.Spacing), Clean(row.Radius));
        Weight = (int)Math.Clamp(row.Weight, 100, 900);
        (Width, Height, Grow) = (Clean(row.Width), Clean(row.Height), Clean(row.Grow));
        (Flags, Border) = (row.Flags, row.Border);
    }

    // Only a number a host can lay out survives: the rest reads as zero.
    static double Clean(double value) => double.IsFinite(value) && value > 0 ? value : 0;

    /// <summary>The control's kind.</summary>
    public RowKind Kind { get; }

    /// <summary>The row it sits in, or <see cref="Rows.None"/> for a root.</summary>
    public int Parent { get; }

    /// <summary>The event it delivers, or a negative number when it delivers none.</summary>
    public long Action { get; }

    /// <summary>Its text, decoded as UTF-8 with invalid bytes replaced.</summary>
    public string Text { get; }

    /// <summary>Its second text: a field's prompt or an option's key.</summary>
    public string Hint { get; }

    /// <summary>Its ink as 0xRRGGBBAA; zero draws nothing.</summary>
    public long Colour { get; }

    /// <summary>Its fill as 0xRRGGBBAA; zero draws none.</summary>
    public long Background { get; }

    /// <summary>Its text size in points; zero keeps the control's natural one.</summary>
    public double Size { get; }

    /// <summary>Its inner padding in points.</summary>
    public double Padding { get; }

    /// <summary>The gap between a container's children in points.</summary>
    public double Spacing { get; }

    /// <summary>Its corner radius in points.</summary>
    public double Radius { get; }

    /// <summary>Its font weight, from 100 to 900.</summary>
    public int Weight { get; }

    /// <summary>Its fixed width in points; zero is natural.</summary>
    public double Width { get; }

    /// <summary>Its fixed height in points; zero is natural.</summary>
    public double Height { get; }

    /// <summary>Its share of the free space beside its siblings; zero is natural.</summary>
    public double Grow { get; }

    /// <summary>Its state flags; unknown bits are ignored.</summary>
    public long Flags { get; }

    /// <summary>Its border colour as 0xRRGGBBAA; zero draws none.</summary>
    public long Border { get; }

    /// <summary>Whether it takes no input.</summary>
    public bool Disabled => (Flags & 1) != 0;

    /// <summary>Whether it is on, chosen or selected.</summary>
    public bool On => (Flags & 2) != 0;

    /// <summary>Whether a field hides its text.</summary>
    public bool Secure => (Flags & 4) != 0;

    /// <summary>Whether its text is set in a fixed-width face.</summary>
    public bool Monospaced => (Flags & 8) != 0;

    /// <summary>Whether its text is slanted.</summary>
    public bool Slanted => (Flags & 16) != 0;

    /// <summary>Whether it is centred in its row.</summary>
    public bool Centred => (Flags & 32) != 0;

    /// <summary>Whether a field delivers only when committed.</summary>
    public bool OnCommit => (Flags & 64) != 0;
}

/// <summary>
/// [NATIVE-SHM-SAFETY] The one reader of a leased view: it walks the row run in place and hands
/// out rows a host may draw. It is a ref struct, so it exists only while the runs' leases are
/// held and nothing can read a run after its lease is released. A row run that is not whole rows
/// is refused; a row of unknown kind, or whose parent is neither -1 nor an earlier row, draws
/// nothing, and so does every row below it; text and hint are read as UTF-8 with invalid bytes
/// replaced, or as empty when they lie outside the text run.
/// </summary>
public ref struct Rows
{
    /// <summary>The parent of a root row, and the action of a control that delivers none.</summary>
    public const int None = -1;

    // Invalid bytes become U+FFFD, exactly as String(decoding:as:) does.
    static readonly UTF8Encoding Decoding = new(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: false);

    readonly ReadOnlySpan<NativeView> rows;
    readonly ReadOnlySpan<byte> text;

    /// <summary>Reads a leased view's rows and the text run they name; the row bytes must be whole rows.</summary>
    public Rows(ReadOnlySpan<byte> rowBytes, ReadOnlySpan<byte> text)
    {
        if (rowBytes.Length % Unsafe.SizeOf<NativeView>() != 0)
        {
            throw new HostException($"The view holds {rowBytes.Length} bytes, which is not whole rows of {Unsafe.SizeOf<NativeView>()}");
        }
        rows = MemoryMarshal.Cast<byte, NativeView>(rowBytes);
        this.text = text;
    }

    /// <summary>The view's control count.</summary>
    public int Count => rows.Length;

    /// <summary>Whether the row at <paramref name="index"/> may be drawn: a kind the host knows, seated in a row that draws, or a root.</summary>
    public bool Draws(int index)
    {
        // A loop, not recursion: a chain of 65,536 nested rows must not exhaust the stack.
        if (index < 0)
        {
            return false;
        }

        for (long row = index; row != None; row = rows[(int)row].Parent)
        {
            if (!Sound((int)row))
            {
                return false;
            }
        }
        return true;
    }

    // A row this host knows how to draw whose parent, if any, is an earlier row.
    bool Sound(int index) =>
        index >= 0 && index < rows.Length
        && rows[index].Kind >= 0 && rows[index].Kind <= (long)RowKind.Sheet
        && rows[index].Parent >= None && rows[index].Parent < index;

    /// <summary>The row at <paramref name="index"/> with every value made safe.</summary>
    public SafeRow At(int index) => new(index, rows[index], Words(rows[index].TextOffset, rows[index].TextLength), Words(rows[index].HintOffset, rows[index].HintLength));

    /// <summary>The rows seated in the row at <paramref name="index"/>, in drawing order.</summary>
    public List<int> Children(int index)
    {
        var seated = new List<int>();
        for (var row = index + 1; row < rows.Length; row++)
        {
            if (rows[row].Parent == index && Draws(row))
            {
                seated.Add(row);
            }
        }
        return seated;
    }

    // The words a row names, or empty once they lie outside the text run: nothing reads past it.
    string Words(long offset, long length) =>
        offset >= 0 && length >= 0 && offset <= text.Length && length <= text.Length - offset
            ? Decoding.GetString(text.Slice((int)offset, (int)length))
            : "";
}
