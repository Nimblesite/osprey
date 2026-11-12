using System.Buffers.Binary;

namespace Osprey.Host.Tests;

/// <summary>
/// [NATIVE-SHM-SAFETY] The safe reader over hand-built byte buffers laid out exactly as
/// native_abi.h shapes its rows: everything a misbehaving DLL could write must read as something
/// the host can draw, never as a crash or a read past a run. These tests need no DLL and run
/// anywhere the tests do.
/// </summary>
public sealed class RowReaderTests
{
    const int Width = 152; // native_abi.h's TD_SHARED_STORAGE(ViewRow, ...)

    // The header's offsets: seven leading words, four fractions, the weight, three fractions, four trailing words.
    static readonly int[] Whole = [0, 8, 16, 24, 32, 40, 48, 88, 120, 128, 136, 144];
    static readonly int[] Part = [56, 64, 72, 80, 96, 104, 112];

    static byte[] Row(long kind = 2, long parent = Rows.None, long action = -1, long textOffset = 0, long textLength = 0,
        long colour = 0, long background = 0, double size = 0, double padding = 0, double spacing = 0, double radius = 0,
        long weight = 400, double width = 0, double height = 0, double grow = 0, long flags = 0, long border = 0,
        long hintOffset = 0, long hintLength = 0)
    {
        var row = new byte[Width];
        var whole = new[] { kind, parent, action, textOffset, textLength, colour, background, weight, flags, border, hintOffset, hintLength };
        var part = new[] { Bits(size), Bits(padding), Bits(spacing), Bits(radius), Bits(width), Bits(height), Bits(grow) };
        foreach (var (value, at) in whole.Zip(Whole))
        {
            BinaryPrimitives.WriteInt64LittleEndian(row.AsSpan(at, 8), value);
        }
        foreach (var (value, at) in part.Zip(Part))
        {
            BinaryPrimitives.WriteInt64LittleEndian(row.AsSpan(at, 8), value);
        }
        return row;
    }

    static long Bits(double value) => BitConverter.DoubleToInt64Bits(value);

    static byte[] Together(params byte[][] rows) => [.. rows.SelectMany(row => row)];

    [Fact]
    public void ARowIsReadAsItWasWritten()
    {
        var words = "Zoë"u8.ToArray();
        var view = new Rows(Together(Row(kind: 0), Row(kind: 2, parent: 0, textOffset: 0, textLength: words.Length, colour: 0x11223344, size: 30, weight: 700, hintOffset: 0, hintLength: 0)), words);
        Assert.Equal(2, view.Count);
        Assert.True(view.Draws(0) && view.Draws(1));
        Assert.Equal((RowKind.Column, Rows.None), (view.At(0).Kind, view.At(0).Parent));
        var label = view.At(1);
        Assert.Equal(("Zoë", RowKind.Label, 0, 30.0, 700, 0x11223344L), (label.Text, label.Kind, label.Parent, label.Size, label.Weight, label.Colour));
    }

    [Fact]
    public void AParentThatIsNotAnEarlierRowDrawsNothing()
    {
        var view = new Rows(Together(Row(kind: 2, parent: -2), Row(kind: 2, parent: 1), Row(kind: 2, parent: 2), Row(kind: 2, parent: 99)), []);
        Assert.False(view.Draws(0)); // a parent below none
        Assert.False(view.Draws(1)); // a row cannot seat itself
        Assert.False(view.Draws(2)); // a parent is always an earlier row
        Assert.False(view.Draws(3)); // a parent beyond the view
        Assert.Equal(Rows.None, view.At(1).Parent); // an unusable parent reads as none
    }

    [Fact]
    public void AChildOfARowThatDrawsNothingDrawsNothing()
    {
        var view = new Rows(Together(Row(kind: 2, parent: 7), Row(kind: 0, parent: 0), Row(kind: 2, parent: 1)), []);
        Assert.False(view.Draws(0));
        Assert.False(view.Draws(1)); // its parent draws nothing
        Assert.False(view.Draws(2)); // and so its parent's parent does neither
    }

    [Fact]
    public void TextOutsideTheTextRunReadsAsEmpty()
    {
        var words = "Hi"u8.ToArray();
        foreach (var (offset, length) in new[] { (2L, 5L), (-1L, 1L), (1L, -3L), (3L, 0L), (long.MaxValue, 2L) })
        {
            Assert.Equal("", new Rows(Row(textOffset: offset, textLength: length), words).At(0).Text);
        }
        Assert.Equal("Hi", new Rows(Row(textOffset: 0, textLength: 2), words).At(0).Text);
    }

    [Fact]
    public void AHintOutsideTheTextRunReadsAsEmpty()
    {
        var view = new Rows(Row(kind: 4, hintOffset: 4, hintLength: 2), "Hi"u8.ToArray());
        Assert.Equal("", view.At(0).Hint);
        var inside = new Rows(Row(kind: 4, hintOffset: 1, hintLength: 1), "Hi"u8.ToArray());
        Assert.Equal("i", inside.At(0).Hint);
    }

    [Fact]
    public void InvalidUtf8IsDecodedWithReplacement()
    {
        var broken = new byte[] { 0x41, 0xC3, 0x28, 0x42 }; // 0xC3 0x28 is not a whole character
        var view = new Rows(Row(textOffset: 0, textLength: broken.Length), broken);
        Assert.Equal("A\uFFFD(B", view.At(0).Text);
    }

    [Fact]
    public void NumbersAHostCannotLayOutReadAsZero()
    {
        var view = new Rows(Row(size: double.NaN, padding: -4, spacing: double.PositiveInfinity, radius: double.NegativeInfinity, width: -1, height: double.NaN, grow: -0.5), []);
        var row = view.At(0);
        Assert.All(new[] { row.Size, row.Padding, row.Spacing, row.Radius, row.Width, row.Height, row.Grow }, value => Assert.Equal(0.0, value));
    }

    [Fact]
    public void AWeightOutsideItsRangeIsClampedIntoIt()
    {
        var view = new Rows(Together(Row(weight: -50), Row(weight: 0), Row(weight: 400), Row(weight: 5000)), []);
        var weights = new int[view.Count];
        for (var row = 0; row < view.Count; row++)
        {
            weights[row] = view.At(row).Weight;
        }
        Assert.Equal([100, 100, 400, 900], weights);
    }

    [Fact]
    public void AKindTheHostDoesNotKnowDrawsNothing()
    {
        var view = new Rows(Together(Row(kind: 17), Row(kind: -1), Row(kind: 16)), []);
        Assert.False(view.Draws(0));
        Assert.False(view.Draws(1));
        Assert.True(view.Draws(2)); // a sheet is the last kind the host draws
    }

    [Fact]
    public void ARunThatIsNotWholeRowsIsRefused()
    {
        Assert.Throws<HostException>(() => new Rows(new byte[100], []));
    }

    [Fact]
    public void ChildrenAreNamedInDrawingOrder()
    {
        var view = new Rows(Together(Row(kind: 0), Row(kind: 2, parent: 0), Row(kind: 2, parent: 0), Row(kind: 2, parent: 3), Row(kind: 2, parent: 3)), []);
        Assert.Equal([1, 2], view.Children(0));
        Assert.Empty(view.Children(3)); // row 4 names it, but draws nothing
        Assert.Empty(view.Children(4));
    }

    [Fact]
    public void FlagsReadAsTheStateTheyName()
    {
        var view = new Rows(Row(flags: 1 | 2 | 4 | 8 | 16 | 32 | 64 | 128), []);
        var row = view.At(0);
        Assert.True(row.Disabled && row.On && row.Secure && row.Monospaced && row.Slanted && row.Centred && row.OnCommit);
        Assert.Equal(1 | 2 | 4 | 8 | 16 | 32 | 64 | 128, row.Flags); // an unknown bit is kept, not read
    }
}
