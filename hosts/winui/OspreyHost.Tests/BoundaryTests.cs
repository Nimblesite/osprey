using System.Text.Json;

namespace Osprey.Host.Tests;

/// <summary>
/// [WINUI-HOST-DLL] The examples compiled to DLLs and driven through the real boundary with no
/// window: the import slots, start, capabilities, host-held slices, streamed scene shapes and
/// dispatch. Everything an application sends must be something this host draws, and nothing may
/// come back as a diagnostic. Build each DLL first with
/// <c>osprey build &lt;example&gt; --target=windows-x64 -o &lt;example&gt;/build/&lt;name&gt;.dll</c>.
/// </summary>
public sealed class BoundaryTests
{
    const int Rounds = 8;

    static string Built => Path.Combine(Framework.Root, "examples", "storage", "build", "storage.dll");

    static string Example(string name) => Path.Combine(Framework.Root, "examples", "gpui", name, "build", $"{name}.dll");

    [Fact]
    public void FlockStreamsItsFlockIntoItsSceneFrameAfterFrame()
    {
        var path = Example("flock");
        Assert.SkipUnless(File.Exists(path), $"No {path}: build it with osprey build examples/gpui/flock --target=windows-x64");
        var regions = new RegionStore();
        var library = Library.Open(path);
        var envelope = Received(library.Start(Events.Context(Events.Now(), 0, Path.GetTempPath(), [])), regions);
        Assert.Contains(envelope.Requests, request => Envelopes.Text(request, "kind") == "animate" && Envelopes.Flag(request, "on"));
        for (var frame = 0; frame < 3; frame++)
        {
            envelope = Received(library.Dispatch(envelope.State, Events.Frame(Events.Now(), 1.0 / 60)), regions);
            var scenes = envelope.Patches.SelectMany(patch => Nodes(patch.Node)).Where(node => Envelopes.Text(node, "kind") == "scene").ToList();
            var streamed = Shapes.Taken.ToList();
            Assert.Equal(streamed.Count, scenes.Sum(scene => Envelopes.Number(scene, "streamed")));
            Assert.True(streamed.Count >= 1000, $"{streamed.Count} shapes streamed for a flock of 1000");
            var taken = 0;
            foreach (var scene in scenes)
            {
                var count = (int)Envelopes.Number(scene, "streamed");
                Assert.NotEmpty(Scenes.SceneDecoder.Decode(scene, streamed.GetRange(taken, count)).Shapes);
                taken += count;
            }
        }
    }

    [Fact]
    public void PaintingDrawsItsPathsAndTakesAStrokeFromThePointer()
    {
        var path = Example("painting");
        Assert.SkipUnless(File.Exists(path), $"No {path}: build it with osprey build examples/gpui/painting --target=windows-x64");
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

    [Fact]
    public async Task StorageMeasuresItsHomeThroughTheRealBoundary()
    {
        Assert.SkipUnless(File.Exists(Built), $"No {Built}: build it with examples/storage/run.sh --host=winui");
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
