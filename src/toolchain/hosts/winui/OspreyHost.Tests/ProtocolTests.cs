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
    public void AControlSendsItsOrdinalAndOneValueAsWords(object? value, string words)
    {
        var touch = Touch.Control(7, Touch.WordsOf(value));
        Assert.Equal((TouchKind.Control, 7L, words, ""), (touch.Kind, touch.Ordinal, touch.Words, touch.Detail));
    }

    [Fact]
    public void ASceneSendsItsKeysAndPointerInItsOwnKind()
    {
        var key = Touch.Key(3, "left", true);
        Assert.Equal((TouchKind.Key, 3L, "left", true), (key.Kind, key.Ordinal, key.Words, key.Down));
        var pointer = Touch.Pointer(3, 12.5, 7.25, true, false);
        Assert.Equal((TouchKind.Pointer, 12.5, 7.25, true, false), (pointer.Kind, pointer.X, pointer.Y, pointer.Down, pointer.Shift));
        var viewed = Touch.Viewed(3, 800, 600);
        Assert.Equal((TouchKind.Viewed, 800.0, 600.0), (viewed.Kind, viewed.X, viewed.Y));
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
    public void AToldTreePastTheDepthBoundIsRefused()
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
    public void TheRegionStoreReplacesChangedRegionsAndDropsTheRest()
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

    /// <summary>A framed component with <paramref name="style"/> as its style record.</summary>
    static Node Framed(Node style, Node body = default) =>
        Node.Of(new Dictionary<string, Node> { ["kind"] = Node.Of("framed"), ["style"] = style, ["body"] = body });

    /// <summary>One member of a record.</summary>
    static Node Style(string member, Node value) => Node.Of(new Dictionary<string, Node> { [member] = value });

    /// <summary>The refusal of one told node, or a message saying it was wrongly accepted.</summary>
    static string ViewportRefusal(Node node)
    {
        try
        {
            Viewports.Refuse("root", node);
        }
        catch (HostException problem)
        {
            return problem.Message;
        }
        return "accepted a frame this host cannot draw";
    }

    [Fact]
    public void AViewportSizedFrameIsRefused()
    {
        foreach (var inset in new[] { 0L, 400L })
        {
            var problem = ViewportRefusal(Framed(Style("viewportHeightInset", Node.Of(inset))));
            Assert.Contains("viewportHeightInset", problem);
            Assert.Contains("-1", problem);
        }
    }

    [Fact]
    public void AScrollingFrameIsRefused()
    {
        var problem = ViewportRefusal(Framed(Style("scrollY", Node.Of(true))));
        Assert.Contains("scrollY", problem);
        Assert.Contains("region root", problem);
    }

    [Fact]
    public void ANestedFrameWithAViewportIsRefused()
    {
        var stack = Node.Of(new Dictionary<string, Node> { ["kind"] = Node.Of("stack"), ["children"] = Node.Of(new List<Node> { Framed(Style("viewportHeightInset", Node.Of(12L))) }) });
        Assert.Contains("viewportHeightInset", ViewportRefusal(stack));
        var body = Framed(Style("scrollY", Node.Of(true)));
        Assert.Contains("scrollY", ViewportRefusal(Framed(Node.Of(new Dictionary<string, Node>()), body)));
    }

    [Fact]
    public void AMalformedViewportFieldIsRefusedNotGuessed()
    {
        foreach (var style in new[] { Style("viewportHeightInset", Node.Of("4")), Style("viewportHeightInset", Node.Of(2.5)), Style("scrollY", Node.Of(1L)), Style("scrollY", Node.Of("true")) })
        {
            Assert.Contains("where it needs", ViewportRefusal(Framed(style)));
        }
        Assert.Contains("an object style", ViewportRefusal(Framed(Node.Of("an object"))));
    }

    [Fact]
    public void OnlyAbsentOrTheContentSizedDefaultIsAccepted()
    {
        Assert.Equal([], new[] { Framed(default), Framed(Style("viewportHeightInset", Node.Of(-1L))), Framed(Style("scrollY", Node.Of(false))), Framed(Node.Of(new Dictionary<string, Node>())) }
            .Select(problem => ViewportRefusal(problem)).Where(problem => problem != "accepted a frame this host cannot draw"));
        foreach (var inset in new[] { -2L, long.MinValue })
        {
            Assert.Contains("viewportHeightInset", ViewportRefusal(Framed(Style("viewportHeightInset", Node.Of(inset)))));
        }
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
