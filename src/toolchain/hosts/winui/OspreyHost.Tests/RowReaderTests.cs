using System.Text;
using Osprey.Host.Scenes;

namespace Osprey.Host.Tests;

/// <summary>
/// [NATIVE-SHM-SAFETY] The one frame reader over hand-built rows laid out exactly as native_abi.h
/// shapes them: nothing a misbehaving DLL could write may reach the host as a wrong value, and a
/// frame that fails any check is refused whole, exactly as the reference host's rows reader refuses
/// it. These tests need no DLL and run anywhere the tests do.
/// </summary>
public sealed class RowReaderTests
{
    internal static NativeView View(long kind = 2, long parent = -1, long action = -1, long textOffset = 0, long textLength = 0,
        long colour = 0, long background = 0, double size = 0, double padding = 0, double spacing = 0, double radius = 0,
        long weight = 400, double width = 0, double height = 0, double grow = 0, long flags = 0, long border = 0,
        long hintOffset = 0, long hintLength = 0) =>
        new(kind, parent, action, textOffset, textLength, colour, background, size, padding, spacing, radius, weight,
            width, height, grow, flags, border, hintOffset, hintLength);

    static NativeAsk Asked(long field, long number = 0, long textOffset = 0, long textLength = 0) =>
        new(field, number, textOffset, textLength);

    internal static byte[] Utf8(string text) => Encoding.UTF8.GetBytes(text);

    internal static Shown Read(NativeView[] views, NativeShape[]? shapes = null, NativeAsk[]? asks = null, byte[]? text = null) =>
        FrameReader.Read(shapes ?? [], views, text ?? [], asks ?? []);

    [Fact]
    public void AFrameReadsItsControlsAndText()
    {
        var words = Utf8("Zoë");
        var shown = Read([View(kind: 0), View(kind: 2, parent: 0, action: 4, textOffset: 0, textLength: words.Length, colour: 0x11223344, size: 30, weight: 700)], text: words);
        Assert.Equal(2, shown.Controls.Count);
        Assert.Equal([0], shown.Roots);
        Assert.Equal((RowKind.Column, (long?)null, 0L), (shown.Controls[0].Kind, shown.Controls[0].Action, shown.Controls[0].Fit.Flags));
        var label = shown.Controls[1];
        Assert.Equal(("Zoë", RowKind.Label, 4L, 30.0, 700.0, 0x11223344u), (label.Words, label.Kind, label.Action, label.Look.Size, label.Look.Weight, label.Look.Ink));
    }

    [Fact]
    public void EveryRootIsListedAndChildrenComeInDrawingOrder()
    {
        var shown = Read([View(kind: 0), View(kind: 2, parent: 0), View(kind: 2, parent: 0), View(kind: 7), View(kind: 2, parent: 3)]);
        Assert.Equal([0, 3], shown.Roots);
        Assert.Equal([1, 2], shown.Controls[0].Children);
        Assert.Equal([4], shown.Controls[3].Children);
    }

    [Fact]
    public void AParentThatIsNotAnEarlierRowIsRefused()
    {
        foreach (var parent in new[] { -2L, 0L, 1L, 2L, 99L, long.MaxValue })
        {
            Assert.Throws<HostException>(() => Read([View(kind: 2, parent: parent), View(kind: 2)]));
        }
    }

    [Fact]
    public void AnUnknownKindIsRefused()
    {
        foreach (var kind in new[] { -1L, 17L, 1000L, long.MinValue })
        {
            Assert.Throws<HostException>(() => Read([View(kind: kind)]));
        }
    }

    [Fact]
    public void AnUnknownFlagIsRefused()
    {
        foreach (var flags in new[] { 128L, 256L, 1L << 20, long.MaxValue })
        {
            Assert.Throws<HostException>(() => Read([View(flags: flags)]));
        }
    }

    [Fact]
    public void TextOutsideTheTextRunIsRefused()
    {
        var words = Utf8("Hi");
        foreach (var (offset, length) in new[] { (2L, 5L), (-1L, 1L), (1L, -3L), (long.MaxValue, 2L) })
        {
            Assert.Throws<HostException>(() => Read([View(textOffset: offset, textLength: length)], text: words));
        }
        Assert.Equal("Hi", Read([View(textOffset: 0, textLength: 2)], text: words).Controls[0].Words);
        Assert.Equal("i", Read([View(kind: 4, hintOffset: 1, hintLength: 1)], text: words).Controls[0].Hint);
        Assert.Throws<HostException>(() => Read([View(kind: 4, hintOffset: 4, hintLength: 2)], text: words));
        Assert.Equal("", Read([View(kind: 4, textOffset: 2, textLength: 0)], text: words).Controls[0].Words);
    }

    [Fact]
    public void InvalidUtf8IsRefused()
    {
        byte[] broken = [0x41, 0xC3, 0x28, 0x42]; // 0xC3 0x28 is not a whole character
        Assert.Throws<HostException>(() => Read([View(textOffset: 0, textLength: broken.Length)], text: broken));
    }

    [Fact]
    public void NumbersAHostCannotLayOutAreRefused()
    {
        Assert.Throws<HostException>(() => Read([View(size: double.NaN)]));
        Assert.Throws<HostException>(() => Read([View(padding: -4)]));
        Assert.Throws<HostException>(() => Read([View(spacing: double.PositiveInfinity)]));
        Assert.Throws<HostException>(() => Read([View(radius: double.NegativeInfinity)]));
        Assert.Throws<HostException>(() => Read([View(width: -1)]));
        Assert.Throws<HostException>(() => Read([View(height: double.NaN)]));
        Assert.Throws<HostException>(() => Read([View(grow: -0.5)]));
        Assert.Equal(0.0, Read([View(width: 0)]).Controls[0].Fit.Width);
    }

    [Fact]
    public void AWeightOutsideItsRangeIsRefused()
    {
        foreach (var weight in new[] { 99L, 0L, 901L, long.MinValue })
        {
            Assert.Throws<HostException>(() => Read([View(weight: weight)]));
        }
        Assert.Equal(400.0, Read([View(weight: 400)]).Controls[0].Look.Weight);
    }

    [Fact]
    public void AColourOutsidePackedRgbaIsRefused()
    {
        foreach (var colour in new[] { -1L, (long)uint.MaxValue + 1, long.MaxValue })
        {
            Assert.Throws<HostException>(() => Read([View(colour: colour)]));
            Assert.Throws<HostException>(() => Read([View(background: colour)]));
            Assert.Throws<HostException>(() => Read([View(border: colour)]));
        }
        Assert.Equal(uint.MaxValue, Read([View(colour: uint.MaxValue)]).Controls[0].Look.Ink);
    }

    [Fact]
    public void FlagsReadAsTheStateTheyName()
    {
        var control = Read([View(flags: RowFlags.All)]).Controls[0];
        Assert.True(control.Fit.Disabled && control.Fit.On && control.Fit.Secure && control.Fit.Monospaced && control.Fit.Slanted && control.Fit.Centred && control.Fit.OnCommit);
        Assert.Equal(RowFlags.All, control.Fit.Flags);
    }

    [Fact]
    public void AFrameWithNoRowsReadsNothing()
    {
        var shown = Read([]);
        Assert.Empty(shown.Controls);
        Assert.Empty(shown.Roots);
        Assert.Empty(shown.Asks);
        Assert.Empty(shown.Scenes);
    }

    [Fact]
    public void AskRowsStartAtFieldZeroAndGatherTheirArguments()
    {
        var words = Utf8("t1foldersecond");
        var shown = Read([View(kind: 0)], asks:
        [
            Asked(0, number: 2, textOffset: 0, textLength: 2),
            Asked(1, textOffset: 2, textLength: 6),
            Asked(1, textOffset: 8, textLength: 6),
            Asked(2, number: 100),
        ], text: words);
        var ask = Assert.Single(shown.Asks);
        Assert.Equal((2L, "t1", "list-directories"), (ask.Code, ask.Ticket, ask.Kind()));
        Assert.Equal(["folder", "second"], ask.Texts(1));
        Assert.Equal(("folder", 100L), (ask.Text(1), ask.Number(2)));
        Assert.True(ask.Flag(2));
        Assert.Equal(["t1"], shown.Requests.Select(request => request.Ticket));
        Assert.Empty(shown.Slices);
        Assert.Empty(shown.Diagnostics);
    }

    [Fact]
    public void AnAskFieldBeforeAnyAskOrBelowZeroIsRefused()
    {
        Assert.Throws<HostException>(() => Read([View(kind: 0)], asks: [Asked(1)]));
        Assert.Throws<HostException>(() => Read([View(kind: 0)], asks: [Asked(0, 1), Asked(-1)]));
    }

    [Fact]
    public void AnAskWithAnUnknownCodeIsRefused()
    {
        foreach (var code in new[] { 0L, 26L, 99L, 102L, -1L })
        {
            Assert.Throws<HostException>(() => Read([View(kind: 0)], asks: [Asked(0, number: code)]));
        }
    }

    [Fact]
    public void SliceWritesAndDiagnosticsAreReadByCode()
    {
        var words = Utf8("keytextwhy");
        var shown = Read([View(kind: 0)], asks:
        [
            Asked(0, number: Ask.SliceWrite, textOffset: 0, textLength: 3),
            Asked(1, textOffset: 3, textLength: 4),
            Asked(0, number: Ask.Diagnostic, textOffset: 7, textLength: 3),
        ], text: words);
        Assert.Equal([("key", "text")], shown.Slices);
        Assert.Equal(["why"], shown.Diagnostics);
        Assert.Empty(shown.Requests);
    }

    [Fact]
    public void PairArgumentsAreReadTwoRowsAtATime()
    {
        var words = Utf8("Accepttext/plainZoe");
        var shown = Read([View(kind: 0)], asks:
        [
            Asked(0, number: 10),
            Asked(3, textOffset: 0, textLength: 6),
            Asked(3, textOffset: 6, textLength: 10),
            Asked(3, textOffset: 16, textLength: 3),
        ], text: words);
        Assert.Equal([("Accept", "text/plain"), ("Zoe", "")], shown.Asks[0].Pairs(3));
    }

    [Fact]
    public void ACapabilityNameIsKnownOnlyForItsCode()
    {
        Assert.Equal("run", new Ask(1, "", []).Kind());
        Assert.Equal("process-stop", new Ask(25, "", []).Kind());
        Assert.Equal("", new Ask(Ask.SliceWrite, "", []).Kind());
        Assert.Equal("", new Ask(Ask.Diagnostic, "", []).Kind());
        Assert.Equal("", new Ask(0, "", []).Kind());
        Assert.Equal(25, Ask.Kinds.Count);
    }

    // [NATIVE-SHM-NODES] One told row: its name is its hint, its weight the type of the value it carries.
    static NativeView Told(long kind, long parent = -1, long weight = 0, long ink = 0, double size = 0,
        long hintOffset = 0, long hintLength = 0, long textOffset = 0, long textLength = 0) =>
        View(kind: kind, parent: parent, weight: weight, colour: ink, size: size, hintOffset: hintOffset, hintLength: hintLength, textOffset: textOffset, textLength: textLength);

    [Fact]
    public void AToldRowCarriesAValueAndNotAColour()
    {
        foreach (var ink in new[] { -1L, long.MaxValue, long.MinValue, (long)uint.MaxValue + 1 })
        {
            var read = Read([Told(33, weight: 1, ink: ink)]).Controls[0];
            Assert.Equal((RowKind.Field, ink), (read.Kind, read.Value));
            Assert.Equal((0u, 0.0), (read.Look.Ink, read.Fit.Width));
        }
        Assert.True(Read([Told(33, weight: 3, ink: 1)]).Controls[0].Value != 0);
        Assert.Equal(4.5, Read([Told(33, weight: 2, size: 4.5)]).Controls[0].Look.Size);
        Assert.Equal(4.5, Read([Told(33, weight: 4, size: 4.5)]).Controls[0].Look.Size);
    }

    [Fact]
    public void AToldWeightOutsideZeroToFourIsRefused()
    {
        foreach (var weight in new[] { -1L, 5L, 100L, 400L, long.MaxValue })
        {
            foreach (var kind in new[] { 33L, 34L, 35L })
            {
                Assert.Throws<HostException>(() => Read([Told(kind, weight: weight)]));
            }
        }
        foreach (var weight in new[] { 0L, 1L, 2L, 3L, 4L })
        {
            Assert.Equal(weight, Read([Told(33, weight: weight)]).Controls[0].Look.Weight);
        }
    }

    [Fact]
    public void EveryToldKindReadsWithItsNameAndItsChildren()
    {
        var words = Utf8("nametext");
        var shown = Read([Told(34, hintOffset: 0, hintLength: 4), Told(33, parent: 0, hintOffset: 4, hintLength: 4, textOffset: 0, textLength: 4), Told(35, parent: 0, hintOffset: 4, hintLength: 4)], text: words);
        Assert.Equal((RowKind.Record, "name"), (shown.Controls[0].Kind, shown.Controls[0].Hint));
        Assert.Equal((RowKind.Field, "text", "name"), (shown.Controls[1].Kind, shown.Controls[1].Hint, shown.Controls[1].Words));
        Assert.Equal(RowKind.Record, shown.Controls[0].Kind);
        Assert.Equal([1, 2], shown.Controls[0].Children);
        Assert.Equal([0], shown.Roots);
    }

    [Fact]
    public void AToldListKeepsItsItemsInOrder()
    {
        var shown = Read([Told(35), Told(33, parent: 0, weight: 1, ink: 1), Told(33, parent: 0, weight: 1, ink: 2), Told(33, parent: 0, weight: 1, ink: 3)]);
        Assert.Equal([1L, 2L, 3L], shown.Controls[0].Children.Select(child => shown.Controls[child].Value));
    }

    [Fact]
    public void AToldRowsTextOutsideTheRunIsRefused()
    {
        var words = Utf8("Hi");
        Assert.Throws<HostException>(() => Read([Told(33, textOffset: 1, textLength: 4)], text: words));
        Assert.Throws<HostException>(() => Read([Told(34, hintOffset: -1, hintLength: 1)], text: words));
    }

    [Fact]
    public void ASceneMayNameAToldRecordRow()
    {
        var words = Utf8("Hi");
        NativeShape[] shapes =
        [
            new(20, 0, 100, 100, 1, 0, 0, 0, 0, 0, 0),
            new(4, 0, 10, 10, 12, 0, 0, 0xFFFFFFFF, 0, 0, 2),
        ];
        var shown = Read([Told(34)], shapes, text: words);
        var scene = Assert.Single(shown.Scenes);
        Assert.Equal(0, scene.Key);
        Assert.Equal("Hi", scene.Value.Shapes.OfType<GlyphsShape>().Single().Text);
    }
}
