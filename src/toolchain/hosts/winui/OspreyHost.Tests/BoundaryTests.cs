using System.Text;
using System.Text.Json;

namespace Osprey.Host.Tests;

/// <summary>
/// [WINUI-HOST-DLL] The examples compiled to DLLs and driven through the real boundary with no
/// window: the import slots, start, capabilities, host-held slices, streamed scene shapes read
/// from the shared frame, dispatch, and one whole shared-view application. Everything an
/// application sends must be something this host draws, and nothing may come back as a diagnostic.
/// Build each DLL first with
/// <c>osprey build &lt;example&gt; --target=windows-x64 -o &lt;example&gt;/build/&lt;name&gt;.dll</c>.
/// </summary>
public sealed class BoundaryTests
{
    const int Rounds = 8;

    static string Built => Path.Combine(Framework.Root, "src", "osprey", "examples", "storage", "build", "storage.dll");

    static string Example(string name) => Path.Combine(Framework.Root, "src", "osprey", "examples", "gpui", name, "build", $"{name}.dll");

    static string Tally => Path.Combine(Framework.Root, "src", "osprey", "examples", "tally", "build", "tally.dll");

    [Fact]
    public void FlockStreamsItsFlockIntoItsSceneFrameAfterFrame()
    {
        var path = Example("flock");
        Assert.SkipUnless(File.Exists(path), $"No {path}: build it with osprey build src/osprey/examples/gpui/flock --target=windows-x64");
        var regions = new RegionStore();
        var library = Library.Open(path);
        var envelope = Received(library.Start(Events.Context(Events.Now(), 0, Path.GetTempPath(), [])), regions);
        Assert.Contains(envelope.Requests, request => Envelopes.Text(request, "kind") == "animate" && Envelopes.Flag(request, "on"));
        for (var frame = 0; frame < 3; frame++)
        {
            envelope = Received(library.Dispatch(envelope.State, Events.Frame(Events.Now(), 1.0 / 60)), regions);
            var scenes = envelope.Patches.SelectMany(patch => Nodes(patch.Node)).Where(node => Envelopes.Text(node, "kind") == "scene").ToList();
            var streamed = library.Shapes!.Rows;
            Assert.Equal(streamed.Length, scenes.Sum(scene => Envelopes.Number(scene, "streamed")));
            Assert.True(streamed.Length >= 1000, $"{streamed.Length} shapes streamed for a flock of 1000");
            var taken = 0;
            foreach (var scene in scenes)
            {
                var count = (int)Envelopes.Number(scene, "streamed");
                Assert.NotEmpty(Scenes.SceneDecoder.Decode(scene, streamed.Slice(taken, count)).Shapes);
                taken += count;
            }
        }
    }

    [Fact]
    public void PaintingDrawsItsPathsAndTakesAStrokeFromThePointer()
    {
        var path = Example("painting");
        Assert.SkipUnless(File.Exists(path), $"No {path}: build it with osprey build src/osprey/examples/gpui/painting --target=windows-x64");
        var regions = new RegionStore();
        var library = Library.Open(path);
        var envelope = Received(library.Start(Events.Context(Events.Now(), 0, Path.GetTempPath(), [])), regions);
        var (key, revision, canvas) = regions.All.SelectMany(region => Nodes(region.Node).Select(node => (region.Key, region.Revision, node))).Single(found => Envelopes.Text(found.node, "kind") == "scene");
        var before = Scenes.SceneDecoder.Decode(canvas, []).Shapes.OfType<Scenes.PathShape>().Count();
        var ordinal = Envelopes.Number(canvas, "event");
        foreach (var (x, y, down) in new[] { (100.0, 400.0, true), (180.0, 430.0, true), (260.0, 420.0, false) })
        {
            envelope = Received(library.Dispatch(envelope.State, Events.Control(Events.Now(), key, regions.Find(key)!.Revision, ordinal, new PointerInput(x, y, down, false))), regions);
        }
        var after = Scenes.SceneDecoder.Decode(Nodes(regions.Find(key)!.Node).Single(node => Envelopes.Text(node, "kind") == "scene"), []);
        Assert.True(after.Shapes.OfType<Scenes.PathShape>().Count() > before, $"{before} paths before the stroke, {after.Shapes.OfType<Scenes.PathShape>().Count()} after");
        Assert.True(revision >= 1);
    }

    // Every JSON object below a node, the node first, in document order.
    static IEnumerable<JsonElement> Nodes(JsonElement node) => node.ValueKind switch
    {
        JsonValueKind.Object => node.EnumerateObject().Select(member => member.Value).SelectMany(Nodes).Prepend(node),
        JsonValueKind.Array => node.EnumerateArray().SelectMany(Nodes),
        _ => [],
    };

    /// <summary>
    /// [NATIVE-SHM-VIEW] Tally driven exactly as its reference host drives it: start, three adds, a
    /// take, an edit to "Zoë", a restyle, a clear and a refused event, every view leased and read in
    /// place from the shared frame with the same assertions the C reference makes.
    /// </summary>
    [Fact]
    public void TallyDrawsItsViewsInPlaceFromTheSharedFrame()
    {
        var path = Tally;
        Assert.SkipUnless(File.Exists(path), $"No {path}: build it with osprey build src/osprey/examples/tally --target=windows-x64");
        var library = Library.Open(path);
        Assert.Equal(0, library.StartView());
        var view = new Shared(library);
        try
        {
            First(view);
            view = Journey(view);
            view = Refused(view);
        }
        finally
        {
            view.Dispose();
        }
    }

    static void First(Shared view)
    {
        long[] kinds = [0, 2, 2, 1, 3, 3, 3, 4, 5, 3], parents = [-1, 0, 0, 0, 3, 3, 3, 0, 0, 0], actions = [-1, -1, -1, -1, 0, 1, 2, 3, -1, 4];
        Assert.Equal(10, view.Count);
        for (var row = 0; row < view.Count; row++)
        {
            Assert.Equal((kinds[row], parents[row], actions[row]), view.Shape(row));
        }
        Assert.Equal("Tally", view.Words(1));
        Assert.Equal("0", view.Words(2));
        Assert.Equal("Add one", view.Words(5));
        Assert.Equal("Use night colours", view.Words(9));
        Assert.Equal((0xFBFAF7FFL, 32.0, 18.0), (view.Row(0).Background, view.Row(0).Padding, view.Row(0).Spacing));
        Assert.Equal((0x0B6E8AFFL, 72.0, 800L), (view.Row(2).Colour, view.Row(2).Size, view.Row(2).Weight));
        Assert.Equal((0x0B6E8AFFL, 10.0), (view.Row(5).Background, view.Row(5).Radius));
        Untouched(view);
        var read = new Rows(view.Rows.Bytes, view.Text.Bytes);
        Assert.Equal(view.Count, read.Count);
        Assert.Equal("Tally", read.At(1).Text);
        Assert.Equal((RowKind.Column, RowKind.Line, RowKind.Press, RowKind.Entry, RowKind.Gap),
            (read.At(0).Kind, read.At(3).Kind, read.At(5).Kind, read.At(7).Kind, read.At(8).Kind));
    }

    // Tally never extends a row: its fixed extents, flex share, state, edge and hint all read as zero.
    static void Untouched(Shared view) =>
        Assert.All(Enumerable.Range(0, view.Count), row => Assert.Equal((0.0, 0.0, 0.0, 0L, 0L, 0L, 0L),
            (view.Row(row).Width, view.Row(row).Height, view.Row(row).Grow, view.Row(row).Flags, view.Row(row).Border, view.Row(row).HintOffset, view.Row(row).HintLength)));

    // Counting, renaming and restyling, each read back from the frame the call wrote.
    static Shared Journey(Shared view)
    {
        view = view.After(5, "");
        view = view.After(5, "");
        view = view.After(5, "");
        Assert.Equal(10, view.Count);
        Assert.Equal("3", view.Words(2));
        view = view.After(4, "");
        Assert.Equal("2", view.Words(2));
        view = view.After(7, "Zoë");
        Assert.Equal(("Zoë's tally", "Zoë", "2"), (view.Words(1), view.Words(7), view.Words(2)));
        var read = new Rows(view.Rows.Bytes, view.Text.Bytes);
        Assert.True(read.Draws(7) && read.At(7).Kind == RowKind.Entry && read.At(7).Text == "Zoë");
        view = view.After(9, "");
        Assert.Equal((0x10141FFFL, 0x7AD7F0FFL), (view.Row(0).Background, view.Row(2).Colour));
        Assert.Equal(("Use day colours", "2"), (view.Words(9), view.Words(2)));
        view = view.After(6, "");
        Assert.Equal("0", view.Words(2));
        return view;
    }

    // An event that names no drawn control changes nothing, and the bridge keeps every byte it held.
    static Shared Refused(Shared view)
    {
        var before = view.Library.Bridge.Usage();
        Assert.NotEqual(0, view.Library.EventView(99, ""));
        Assert.NotEqual(0, view.Library.EventView(-1, "x"));
        Assert.Equal(before.Bytes, view.Library.Bridge.Usage().Bytes);
        Assert.Equal(("0", "Use day colours"), (view.Words(2), view.Words(9)));
        return view;
    }

    /// <summary>One leased view of a shared application: its rows and their text, released once the next view is leased.</summary>
    sealed class Shared(Library library) : IDisposable
    {
        public Library Library { get; } = library;

        public Run<NativeView> Rows { get; private set; } = library.Bridge.Take<NativeView>(Bridge.ViewsRun);

        public Run<byte> Text { get; private set; } = library.Bridge.Take<byte>(Bridge.TextRun);

        public int Count => Rows.Rows.Length;

        public NativeView Row(int index) => Rows.Rows[index];

        public (long Kind, long Parent, long Action) Shape(int index) => (Rows.Rows[index].Kind, Rows.Rows[index].Parent, Rows.Rows[index].Action);

        public string Words(int index) => Encoding.UTF8.GetString(Text.Bytes.Slice((int)Row(index).TextOffset, (int)Row(index).TextLength));

        // Delivers the control numbered `row` with its words, then leases the next view before this one is released.
        public Shared After(int row, string words)
        {
            Assert.Equal(0, Library.EventView(Rows.Rows[row].Action, words));
            var next = new Shared(Library);
            Dispose();
            return next;
        }

        public void Dispose()
        {
            Rows.Dispose();
            Text.Dispose();
        }
    }

    [Fact]
    public async Task StorageMeasuresItsHomeThroughTheRealBoundary()
    {
        Assert.SkipUnless(File.Exists(Built), $"No {Built}: build it with src/osprey/examples/storage/run.sh --host=winui");
        var home = Directory.CreateTempSubdirectory("osprey-home-");
        try
        {
            File.WriteAllBytes(Path.Combine(home.FullName, "big.bin"), new byte[64 * 1024]);
            var regions = new RegionStore();
            var library = Library.Open(Built);
            var envelope = Received(library.Start(Events.Context(Events.Now(), 0, home.FullName, [])), regions);
            Assert.Equal("measure-folders", Envelopes.Text(Assert.Single(envelope.Requests), "kind"));
            var walked = await Settled(library, envelope, regions);
            Assert.Empty(walked.Requests);
            Assert.NotNull(regions.Find(RegionStore.Root));
            Assert.Contains(regions.All, region => region.Node.GetRawText().Contains("big.bin", StringComparison.Ordinal));
        }
        finally
        {
            home.Delete(recursive: true);
        }
    }

    // Answers every request, oldest first, dispatching each completion against the newest state.
    static async Task<Envelope> Settled(Library library, Envelope envelope, RegionStore regions)
    {
        var capabilities = new Capabilities(new Pickers(() => Task.FromResult<string?>(null), _ => Task.FromResult<string?>(null)), new Store(Path.Combine(Path.GetTempPath(), "osprey-store-unused")));
        var pending = new Queue<JsonElement>(envelope.Requests);
        for (var round = 0; round < Rounds && pending.TryDequeue(out var request); round++)
        {
            var completion = await capabilities.Answer(request);
            foreach (var write in completion.Writes)
            {
                Slices.Put(write.Key, write.Text);
            }
            envelope = Received(library.Dispatch(envelope.State, Events.Completed(Events.Now(), completion)), regions);
            foreach (var next in envelope.Requests)
            {
                pending.Enqueue(next);
            }
        }
        Assert.Empty(pending);
        return envelope;
    }

    static Envelope Received(string text, RegionStore regions)
    {
        var envelope = Envelopes.Parse(text);
        Assert.Empty(envelope.Diagnostics);
        foreach (var write in envelope.Slices)
        {
            Slices.Put(write.Key, write.Text);
        }
        regions.Apply(envelope);
        Assert.All(envelope.Patches.SelectMany(patch => Kinds(patch.Node)), kind => Assert.Contains(kind, Coverage.Drawn));
        return envelope;
    }

    // Every node kind in a tree: each JSON object with a `kind` member is a node.
    static IEnumerable<string> Kinds(JsonElement node) => node.ValueKind switch
    {
        JsonValueKind.Object => node.EnumerateObject().SelectMany(Kinds),
        JsonValueKind.Array => node.EnumerateArray().SelectMany(Kinds),
        _ => [],
    };

    static IEnumerable<string> Kinds(JsonProperty member) =>
        member.Name == "kind" && member.Value.ValueKind == JsonValueKind.String ? [member.Value.GetString() ?? ""] : Kinds(member.Value);
}
