using System.Text.Json;

namespace Osprey.Host.Tests;

/// <summary>
/// [WINUI-HOST-DLL] Osprey Storage compiled to a DLL and driven through the real boundary with no
/// window: the import slots, start, the folder walk, the host-held slice and dispatch. Everything
/// the application sends must be something this host draws, and nothing may come back as a
/// diagnostic. Build the DLL first with <c>examples/storage/run.sh --host=winui</c>, or
/// <c>osprey build examples/storage --target=windows-x64 -o examples/storage/build/storage.dll</c>.
/// </summary>
public sealed class BoundaryTests
{
    const int Rounds = 8;

    static string Built => Path.Combine(Framework.Root, "examples", "storage", "build", "storage.dll");

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
        var capabilities = new Capabilities(() => Task.FromResult<string?>(null));
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
