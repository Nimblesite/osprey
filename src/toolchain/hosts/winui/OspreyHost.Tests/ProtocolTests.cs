using System.Text;

namespace Osprey.Host.Tests;

/// <summary>
/// [NATIVE-SHM-EVENT], [UI-REGIONS], [NATIVE-SHM-NODES] and [UI-PROTOCOL]: a control event carries
/// the ordinal the frame issued and one value as words, the region store keeps every region a frame
/// did not resend, a told frame reads its theme and its regions from its rows, and a frame that asks
/// for a viewport this host does not draw is refused whole. Main's envelope tests have no subject
/// any more: the application tells typed rows in the shared frame, and there is no envelope.
/// </summary>
public sealed class ProtocolTests
{
    [Theory]
    [InlineData(null, "")]
    [InlineData("D:\\Media", "D:\\Media")]
    [InlineData(true, "true")]
    [InlineData(false, "false")]
    [InlineData(420L, "420")]
    [InlineData(-7L, "-7")]
    public void AControlSendsItsOrdinalAndOneValue(object? value, string words)
    {
        var touch = Touch.Control(7, Touch.WordsOf(value));
        Assert.Equal((TouchKind.Control, 7L, words, ""), (touch.Kind, touch.Ordinal, touch.Words, touch.Detail));
    }

    [Fact]
    public void AControlSendsItsOrdinalAndOneValueExtraWholeNumberEdges()
    {
        Assert.Equal("420", Touch.WordsOf(420));
        Assert.Equal(long.MinValue.ToString(System.Globalization.CultureInfo.InvariantCulture), Touch.WordsOf(long.MinValue));
        Assert.Equal(long.MaxValue.ToString(System.Globalization.CultureInfo.InvariantCulture), Touch.WordsOf(long.MaxValue));
        Assert.Throws<HostException>(() => Touch.WordsOf(1.5));
        Assert.Throws<HostException>(() => Touch.WordsOf(new byte[1]));
    }

    [Fact]
    public void ASceneSendsItsKeysAndPointerAsObjects()
    {
        var key = Touch.Key(3, "left", true);
        Assert.Equal((TouchKind.Key, 3L, "left", true), (key.Kind, key.Ordinal, key.Words, key.Down));
        var pointer = Touch.Pointer(3, 12.5, 7.25, true, false);
        Assert.Equal((TouchKind.Pointer, 12.5, 7.25, true, false), (pointer.Kind, pointer.X, pointer.Y, pointer.Down, pointer.Shift));
        var viewed = Touch.Viewed(3, 800, 600);
        Assert.Equal((TouchKind.Viewed, 800.0, 600.0), (viewed.Kind, viewed.X, viewed.Y));
    }

    [Fact]
    public void ASceneSendsItsKeysAndPointerAsObjectsExtraReleasedKeysAndFractionalSizes()
    {
        var up = Touch.Key(3, "left", false);
        Assert.Equal((TouchKind.Key, "left", false), (up.Kind, up.Words, up.Down));
        var viewed = Touch.Viewed(3, 320.5, 240.25);
        Assert.Equal((TouchKind.Viewed, 320.5, 240.25, 3L), (viewed.Kind, viewed.X, viewed.Y, viewed.Ordinal));
        var pointer = Touch.Pointer(3, -1, 0, false, true);
        Assert.Equal((TouchKind.Pointer, -1.0, 0.0, false, true), (pointer.Kind, pointer.X, pointer.Y, pointer.Down, pointer.Shift));
    }

    [Fact]
    public void AFrameCarriesTheSecondsSinceThePreviousOne()
    {
        var frame = Touch.Frame(0.016);
        Assert.Equal((TouchKind.Frame, 0.016), (frame.Kind, frame.X));
    }

    [Fact]
    public void AControlCannotSendAValueTheFrameworkDoesNotRead() =>
        Assert.Throws<HostException>(() => Touch.WordsOf(1.5));

    [Fact]
    public void ACompletionCarriesItsResultOrItsError()
    {
        var done = Touch.Completed(Completion.Done("walk:1", """{"path":"D:\\"}"""));
        var failed = Touch.Completed(Completion.Failed("walk:2", "cancelled"));
        Assert.Equal((TouchKind.Completed, "walk:1", true, """{"path":"D:\\"}"""), (done.Kind, done.Words, done.Down, done.Detail));
        Assert.Equal((TouchKind.Completed, "walk:2", false, "cancelled"), (failed.Kind, failed.Words, failed.Down, failed.Detail));
    }

    [Fact]
    public void ALifecyclePhaseIsSentByName()
    {
        var active = Touch.Phase("active");
        Assert.Equal(("active", TouchKind.Phase), (active.Words, active.Kind));
    }

    [Fact]
    public void AChangeOfAppearanceIsSentAsAFlag()
    {
        var dark = Touch.Appearance(dark: true);
        Assert.Equal((TouchKind.Appearance, true), (dark.Kind, dark.Down));
        Assert.False(Touch.Appearance(dark: false).Down);
    }

    /// <summary>The refusal of one frame of rows, or the exception that refused it.</summary>
    static HostException RefusedBy(NativeView[] views, NativeAsk[]? asks = null, byte[]? text = null) =>
        Assert.Throws<HostException>(() => RowReaderTests.Read(views, asks: asks, text: text ?? []));

    [Fact]
    public void AnEnvelopeDecodesEveryMember()
    {
        var words = RowReaderTests.Utf8("Listingwalk:1C:\\Mediawalk[1,2]late event");
        NativeView[] views = [RowReaderTests.View(kind: 13), RowReaderTests.View(kind: 2, parent: 0, textOffset: 0, textLength: 7)];
        NativeShape[] shapes = [new(20, 0, 434, 557, 0.5, 0, 0, 0, 0, 0, 0)];
        NativeAsk[] asks =
        [
            new NativeAsk(0, 17, 7, 6),
            new NativeAsk(1, 0, 13, 8),
            new NativeAsk(0, 100, 21, 4),
            new NativeAsk(1, 0, 25, 5),
            new NativeAsk(0, 101, 30, 10),
        ];
        var shown = RowReaderTests.Read(views, shapes, asks, words);
        Assert.Equal(2, shown.Controls.Count);
        Assert.Equal([0], shown.Roots);
        Assert.Equal((RowKind.Canvas, "Listing"), (shown.Controls[0].Kind, shown.Controls[1].Words));
        var scene = shown.Scenes[0];
        Assert.Equal((434.0, 557.0, 0.5), (scene.Width, scene.Height, scene.Zoom));
        var request = Assert.Single(shown.Requests);
        Assert.Equal(("reveal", "walk:1", "C:\\Media"), (request.Kind(), request.Ticket, request.Text(1)));
        Assert.Equal([("walk", "[1,2]")], shown.Slices);
        Assert.Equal(["late event"], shown.Diagnostics);
    }

    [Fact]
    public void AnEnvelopeDecodesEveryMemberExtraArgumentsByPosition()
    {
        var words = RowReaderTests.Utf8("run:1cmd.exe--verbose--fast");
        NativeAsk[] asks =
        [
            new NativeAsk(0, 1, 0, 5),
            new NativeAsk(1, 0, 5, 7),
            new NativeAsk(2, 0, 12, 9),
            new NativeAsk(2, 0, 21, 6),
            new NativeAsk(3, 5000, 0, 0),
            new NativeAsk(4, 65536, 0, 0),
        ];
        var ask = Assert.Single(RowReaderTests.Read([], [], asks, words).Requests);
        Assert.Equal("run", ask.Kind());
        Assert.Equal("cmd.exe", ask.Text(1));
        Assert.Equal(["--verbose", "--fast"], ask.Texts(2));
        Assert.Equal((5000L, 65536L), (ask.Number(3), ask.Number(4)));
        Assert.True(ask.Flag(4));
    }

    [Fact]
    public void AnotherProtocolVersionIsRefused()
    {
        // The frame's protocol is its codes: an ask code past the twenty-five the framework defines
        // is from a protocol this host does not speak, so it is refused by name, never guessed. The
        // bridge's layout fingerprint is the other half: NativeAbiTests refuses a differing one.
        Assert.Contains("unknown capability code", RefusedBy([], [new NativeAsk(0, 26, 0, 5)], RowReaderTests.Utf8("later")).Message, StringComparison.Ordinal);
        var words = RowReaderTests.Utf8("walk[1,2]late");
        var shown = RowReaderTests.Read([], [], [new NativeAsk(0, 100, 0, 4), new NativeAsk(1, 0, 4, 5), new NativeAsk(0, 101, 9, 4)], words);
        Assert.Equal([("walk", "[1,2]")], shown.Slices);
        Assert.Equal(["late"], shown.Diagnostics);
    }

    [Fact]
    public void AnotherProtocolVersionIsRefusedExtraEveryCodePastTheTwentyFive()
    {
        foreach (var code in new[] { 27L, 28L, 32L, 99L, 102L, long.MaxValue })
        {
            Assert.Throws<HostException>(() => RowReaderTests.Read([], [], [new NativeAsk(0, code, 0, 0)], []));
        }
    }

    [Fact]
    public void AMemberOfTheWrongKindIsRefusedNotGuessed()
    {
        Assert.Contains("unknown kind 17", RefusedBy([RowReaderTests.View(kind: 17)]).Message, StringComparison.Ordinal);
        Assert.Contains("unknown kind 18", RefusedBy([RowReaderTests.View(kind: 18)]).Message, StringComparison.Ordinal);
        Assert.Contains("a told value of an unknown type 5", RefusedBy([RowReaderTests.View(kind: 33, weight: 5)]).Message, StringComparison.Ordinal);
        Assert.Contains("a font weight outside 100 to 900", RefusedBy([RowReaderTests.View(weight: 99)]).Message, StringComparison.Ordinal);
        Assert.Contains("unknown flags 128", RefusedBy([RowReaderTests.View(flags: 128)]).Message, StringComparison.Ordinal);
        Assert.Contains("Ask row before any ask", RefusedBy([], [new NativeAsk(1, 0, 0, 0)]).Message, StringComparison.Ordinal);
        Assert.Contains("Ask row before any ask", RefusedBy([], [new NativeAsk(-1, 0, 0, 0)]).Message, StringComparison.Ordinal);
    }

    [Fact]
    public void AMemberOfTheWrongKindIsRefusedNotGuessedExtraViewRowMembers()
    {
        Assert.Contains("names a parent that is not an earlier row", RefusedBy([RowReaderTests.View(kind: 2, parent: -2)]).Message, StringComparison.Ordinal);
        Assert.Contains("a colour outside 0xRRGGBBAA", RefusedBy([RowReaderTests.View(colour: -1)]).Message, StringComparison.Ordinal);
        Assert.Contains("an extent that is negative or not finite", RefusedBy([RowReaderTests.View(size: -1)]).Message, StringComparison.Ordinal);
        Assert.Contains("an extent that is negative or not finite", RefusedBy([RowReaderTests.View(width: double.NaN)]).Message, StringComparison.Ordinal);
        Assert.Contains("names a text range outside the frame", RefusedBy([RowReaderTests.View(textOffset: 1, textLength: 5)], text: RowReaderTests.Utf8("Hi")).Message, StringComparison.Ordinal);
    }

    [Fact]
    public void TheLaunchContextNamesWindowsAndKeepsTextReadable()
    {
        // The launch context is the typed `open` call: the platform's name and the home folder are
        // borrowed UTF-8 strings, and every argument after the DLL reaches the application in order.
        var launch = Launch.Parse(["--headless", @"C:\Apps\flock.dll", "--elevated", "Zoë's list"]);
        Assert.Equal((@"C:\Apps\flock.dll", true), (launch!.Library, launch.Headless));
        Assert.Equal(["--elevated", "Zoë's list"], launch.Arguments);
        var session = File.ReadAllText(Path.Combine(Framework.Host, "OspreyHost", "Session.cs"));
        Assert.Contains("Platform = \"windows\"", session, StringComparison.Ordinal);
        Assert.Contains("Environment.SpecialFolder.UserProfile", session, StringComparison.Ordinal);
        Assert.Contains("GetUtcOffset", session, StringComparison.Ordinal);
    }

    [Fact]
    public void TheLaunchContextNamesWindowsAndKeepsTextReadableExtraSelfTestFlags()
    {
        Assert.Null(Launch.Parse([]));
        var self = Launch.Parse(["--self-test", "script.json", "shots", "flock.dll", "--elevated"]);
        Assert.Equal(("flock.dll", "script.json", "shots", false), (self!.Library, self.Script, self.Shots, self.Headless));
        Assert.Equal(["--elevated"], self.Arguments);
    }

    [Fact]
    public void AToldFrameReadsItsThemeAndItsRegions()
    {
        var rows = new ToldRows();
        var theme = rows.Record(-1, "theme");
        var tokens = rows.List(theme, "tokens");
        var accent = rows.Record(tokens, "");
        rows.Field(accent, "name", "accent");
        rows.Field(accent, "light", "#0050B3");
        rows.Field(accent, "dark", "#7AC3FF");
        var spacing = rows.List(theme, "spacing");
        rows.Count(spacing, "", 4);
        rows.Count(spacing, "", 8);
        rows.Count(theme, "feedback", 200);
        rows.Count(theme, "settle", 300);
        var region = rows.Record(-1, "region");
        rows.Field(region, "key", "root");
        var node = rows.Record(region, "node");
        rows.Field(node, "kind", "text");
        rows.Field(node, "text", "Hi");

        var frame = Told.Frame(rows.Read());
        Assert.NotNull(frame);
        var one = Assert.Single(frame!.Regions);
        Assert.Equal(("root", "text", "Hi"), (one.Key, one.Node.Text("kind"), one.Node.Text("text")));
        var read = Assert.IsType<Theme>(frame.Theme);
        Assert.Equal(("accent", "#0050B3", "#7AC3FF"), (Assert.Single(read.Tokens).Name, read.Tokens[0].Light, read.Tokens[0].Dark));
        Assert.Equal([4.0, 8.0], read.Spacing);
        Assert.Equal((200L, 300L), (read.Feedback, read.Settle));
    }

    [Fact]
    public void AToldFrameKeepsItsRegionsInOrderAndTheirWholeTrees()
    {
        var rows = new ToldRows();
        var first = rows.Record(-1, "region");
        rows.Field(first, "key", "root");
        var slot = rows.Record(first, "node");
        rows.Field(slot, "kind", "slot");
        rows.Field(slot, "key", "listing");
        var second = rows.Record(-1, "region");
        rows.Field(second, "key", "listing");
        var stack = rows.Record(second, "node");
        rows.Field(stack, "kind", "stack");
        rows.Field(stack, "axis", "vertical");
        var children = rows.List(stack, "children");
        var text = rows.Record(children, "");
        rows.Field(text, "kind", "text");
        rows.Field(text, "text", "Zoë");
        var button = rows.Record(children, "");
        rows.Field(button, "kind", "button");
        rows.Field(button, "label", "Add");
        rows.Flag(button, "enabled", true);
        rows.Count(button, "event", 3);

        var frame = Assert.IsType<ToldFrame>(Told.Frame(rows.Read()));
        Assert.Equal(["root", "listing"], frame.Regions.Select(region => region.Key));
        Assert.Equal(("slot", "listing"), (frame.Regions[0].Node.Text("kind"), frame.Regions[0].Node.Text("key")));
        var stackNode = frame.Regions[1].Node;
        Assert.Equal(("stack", "vertical"), (stackNode.Text("kind"), stackNode.Text("axis")));
        Assert.Equal(["text", "button"], stackNode.Items("children").Select(child => child.Text("kind")));
        Assert.Equal(("Zoë", "Add"), (stackNode.Items("children")[0].Text("text"), stackNode.Items("children")[1].Text("label")));
        Assert.Equal((true, 3L), (stackNode.Items("children")[1].Flag("enabled"), stackNode.Items("children")[1].Number("event")));
    }

    [Fact]
    public void AFrameOfPlainViewRowsTellsNoComponents()
    {
        var shown = RowReaderTests.Read([RowReaderTests.View(kind: 0), RowReaderTests.View(kind: 2, parent: 0)]);
        Assert.Null(Told.Frame(shown));
    }

    [Fact]
    public void ATreeDeeperThanTheBoundIsRefused()
    {
        var rows = new ToldRows();
        var region = rows.Record(-1, "region");
        rows.Field(region, "key", "root");
        var nested = rows.Record(region, "node");
        for (var level = 0; level <= Told.Depth; level++)
        {
            nested = rows.Record(nested, "body");
        }
        Assert.Contains("nests deeper", Assert.Throws<HostException>(() => Told.Frame(rows.Read())).Message);
    }

    [Fact]
    public void ATreeDeeperThanTheBoundIsRefusedExtraATreeAtTheBoundIsDrawn()
    {
        var rows = new ToldRows();
        var region = rows.Record(-1, "region");
        rows.Field(region, "key", "root");
        var nested = rows.Record(region, "node");
        for (var level = 0; level < Told.Depth - 1; level++)
        {
            nested = rows.Record(nested, "body");
        }
        var frame = Assert.IsType<ToldFrame>(Told.Frame(rows.Read()));
        Assert.Equal("root", Assert.Single(frame.Regions).Key);
    }

    [Fact]
    public void TwoToldTreesAreTheSameValueMemberByMember()
    {
        static Node Build(string words) => Node.Of(new Dictionary<string, Node>
        {
            ["kind"] = Node.Of("stack"),
            ["children"] = Node.Of(new List<Node> { Node.Of(words), Node.Of(new Dictionary<string, Node> { ["on"] = Node.Of(true), ["at"] = Node.Of(3L) }) }),
        });
        Assert.Equal(Build("a"), Build("a"));
        Assert.NotEqual(Build("a"), Build("b"));
        Assert.NotEqual(Node.Of(3L), Node.Of(3.0));
        Assert.Equal(Node.Of(3L), Node.Of(3L));
        Assert.NotEqual(Node.Of("3"), Node.Of(3L));
    }

    [Fact]
    public void AToldRecordWithAScenesRowsCarriesItsRow()
    {
        var rows = new ToldRows();
        var region = rows.Record(-1, "region");
        rows.Field(region, "key", "root");
        var scene = rows.Record(region, "node");
        rows.Field(scene, "kind", "scene");
        rows.Real(scene, "width", 320);
        rows.Count(scene, "event", 2);
        NativeShape[] shapes = [new(20, 2, 320, 200, 1, 0, 0, 0, 0, 0, 0)];

        var frame = Assert.IsType<ToldFrame>(Told.Frame(RowReaderTests.Read([.. rows.Views], shapes, text: rows.Text)));
        Assert.Equal(2L, Assert.Single(frame.Regions).Node.Number("sceneRow"));
        Assert.Equal(320.0, frame.Regions[0].Node.Real("width"));
    }

    [Fact]
    public void ARegionThatDidNotChangeKeepsItsTreeAndItsRevision()
    {
        var store = new RegionStore();
        Assert.Equal(["root", "listing"], store.Apply(new ToldFrame(null, [("root", Slot("listing")), ("listing", Kind("empty"))])));
        Assert.Equal([], store.Apply(new ToldFrame(null, [("root", Slot("listing")), ("listing", Kind("empty"))])));
        Assert.Equal(0, store.Find(RegionStore.Root)?.Revision);
        Assert.Equal(0, store.Find("listing")?.Revision);
    }

    [Fact]
    public void TheRegionStoreReplacesPatchedRegionsAndKeepsTheRest()
    {
        var store = new RegionStore();
        store.Apply(new ToldFrame(null, [("root", Slot("listing")), ("listing", Kind("empty"))]));
        var changed = store.Apply(new ToldFrame(null, [("root", Slot("listing")), ("listing", Kind("rows"))]));
        Assert.Equal(["listing"], changed);
        Assert.Equal(0, store.Find(RegionStore.Root)?.Revision);
        Assert.Equal(("rows", 1L), (store.Find("listing")!.Node.Text("kind"), store.Find("listing")!.Revision));
        Assert.Equal(["listing"], store.Apply(new ToldFrame(null, [("root", Slot("listing"))])));
        Assert.Null(store.Find("listing"));
        Assert.Single(store.All);
        Assert.Equal(("root", 0L), (store.Find(RegionStore.Root)!.Key, store.Find(RegionStore.Root)!.Revision));
    }

    [Fact]
    public void TheRegionStoreReplacesPatchedRegionsAndKeepsTheRestExtraADroppedRegionComesBack()
    {
        var store = new RegionStore();
        store.Apply(new ToldFrame(null, [("root", Slot("listing")), ("listing", Kind("rows"))]));
        Assert.Equal(["listing"], store.Apply(new ToldFrame(null, [("root", Slot("listing"))])));
        var back = store.Apply(new ToldFrame(null, [("root", Slot("listing")), ("listing", Kind("rows"))]));
        Assert.Equal(["listing"], back);
        Assert.Equal(("rows", 0L), (store.Find("listing")!.Node.Text("kind"), store.Find("listing")!.Revision));
        Assert.Equal(["listing"], store.Apply(new ToldFrame(null, [("root", Slot("listing")), ("listing", Kind("stack"))])));
        Assert.Equal(("stack", 1L), (store.Find("listing")!.Node.Text("kind"), store.Find("listing")!.Revision));
    }

    /// <summary>A slot component naming another region.</summary>
    static Node Slot(string key) => Node.Of(new Dictionary<string, Node> { ["kind"] = Node.Of("slot"), ["key"] = Node.Of(key) });

    /// <summary>A component of one kind.</summary>
    static Node Kind(string kind) => Node.Of(new Dictionary<string, Node> { ["kind"] = Node.Of(kind) });

    /// <summary>Rows laid out exactly as told.ospml writes them: every name and every value in one text run.</summary>
    sealed class ToldRows
    {
        readonly List<NativeView> rows = [];
        readonly StringBuilder text = new();

        /// <summary>The run's rows, for a frame that also needs scene rows.</summary>
        public NativeView[] Views => [.. rows];

        /// <summary>The run's bytes.</summary>
        public byte[] Text => Encoding.UTF8.GetBytes(text.ToString());

        /// <summary>A drawn frame of these rows, checked as the frame reader checks one.</summary>
        public Shown Read() => RowReaderTests.Read(Views, text: Text);

        /// <summary>A record, or a component when its kind member follows.</summary>
        public int Record(int parent, string hint) => Row(RowKind.Record, parent, hint, 0, 0, 0);

        /// <summary>A list of items.</summary>
        public int List(int parent, string hint) => Row(RowKind.List, parent, hint, 0, 0, 0);

        /// <summary>A text value.</summary>
        public int Field(int parent, string hint, string words)
        {
            var at = Words(words);
            return Row(RowKind.Field, parent, hint, at, Encoding.UTF8.GetByteCount(words), RowValue.Text);
        }

        /// <summary>A whole number.</summary>
        public int Count(int parent, string hint, long value) => Row(RowKind.Field, parent, hint, 0, 0, RowValue.Whole, value);

        /// <summary>A real.</summary>
        public int Real(int parent, string hint, double value) => Row(RowKind.Field, parent, hint, 0, 0, RowValue.Real, 0, value);

        /// <summary>A flag.</summary>
        public int Flag(int parent, string hint, bool on) => Row(RowKind.Field, parent, hint, 0, 0, RowValue.Flag, on ? 1 : 0);

        long bytes;

        long Words(string words)
        {
            var at = bytes;
            text.Append(words);
            bytes += Encoding.UTF8.GetByteCount(words);
            return at;
        }

        int Row(RowKind kind, int parent, string hint, long wordsAt, long wordsLength, long weight, long ink = 0, double size = 0)
        {
            var hintAt = Words(hint);
            rows.Add(new NativeView((long)kind, parent, -1, wordsAt, wordsLength, ink, 0, size, 0, 0, 0, weight, 0, 0, 0, 0, 0, hintAt, Encoding.UTF8.GetByteCount(hint)));
            return rows.Count - 1;
        }
    }
}
