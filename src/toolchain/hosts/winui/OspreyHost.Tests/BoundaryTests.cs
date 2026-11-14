using System.Runtime.Versioning;
using Osprey.Host.Scenes;

namespace Osprey.Host.Tests;

/// <summary>
/// [WINUI-HOST-DLL] The examples compiled to DLLs and driven through the real boundary with no
/// window: the import slots, the three typed exports, capabilities, host-held slices, scene rows read
/// from the shared frame, and one whole shared-view application. Everything an application sends
/// must be something this host draws, and nothing may come back as a diagnostic.
/// Build each DLL first with
/// <c>osprey build &lt;example&gt; --target=windows-x64 -o &lt;example&gt;/build/&lt;name&gt;.dll</c>.
/// </summary>
[SupportedOSPlatform("windows")]
public sealed class BoundaryTests
{
    const int Rounds = 8;

    static string Built => Path.Combine(Framework.Root, "src", "osprey", "examples", "storage", "build", "storage.dll");

    static string Example(string name) => Path.Combine(Framework.Root, "src", "osprey", "examples", "gpui", name, "build", $"{name}.dll");

    static string Tally => Path.Combine(Framework.Root, "src", "osprey", "examples", "tally", "build", "tally.dll");

    static void OnWindows(string path)
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "an application DLL is a Windows binary");
        Assert.SkipUnless(File.Exists(path), $"No {path}: build it first");
    }

    [Fact]
    public void FlockStreamsItsFlockIntoItsSceneFrameAfterFrame()
    {
        var path = Example("flock");
        OnWindows(path);
        var library = Library.Open(path);
        var shown = Received(library.Open(Now(), 0, "windows", Path.GetTempPath(), false));
        Assert.Contains(shown.Requests, ask => ask.Kind() == "animate" && ask.Flag(1));
        Assert.NotEmpty(shown.Scenes);
        Assert.True(shown.Scenes.Values.Sum(scene => scene.Shapes.Count) >= 1000, $"{shown.Scenes.Values.Sum(scene => scene.Shapes.Count)} shapes streamed for a flock of 1000");
        for (var frame = 0; frame < 3; frame++)
        {
            shown = Received(library.Event(Now(), Touch.Frame(1.0 / 60)));
            Assert.NotEmpty(shown.Scenes);
            Assert.True(shown.Scenes.Values.Sum(scene => scene.Shapes.Count) >= 1000);
        }
    }

    [Fact]
    public void PaintingDrawsItsPathsAndTakesAStrokeFromThePointer()
    {
        var path = Example("painting");
        OnWindows(path);
        var library = Library.Open(path);
        var shown = Received(library.Open(Now(), 0, "windows", Path.GetTempPath(), false));
        var canvas = shown.Scenes.Single();
        var ordinal = SceneOrdinal(shown, canvas.Key);
        var before = canvas.Value.Shapes.OfType<PathShape>().Count();
        foreach (var (x, y, down) in new[] { (100.0, 400.0, true), (180.0, 430.0, true), (260.0, 420.0, false) })
        {
            shown = Received(library.Event(Now(), Touch.Pointer(ordinal, x, y, down, false)));
        }
        var after = shown.Scenes[canvas.Key].Shapes.OfType<PathShape>().Count();
        Assert.True(after > before, $"{before} paths before the stroke, {after} after");
    }

    // [NATIVE-SHM-NODES] The ordinal the told scene record carries, found by the row its scene rows name.
    static long SceneOrdinal(Shown shown, int row) =>
        Records(Assert.IsType<ToldFrame>(Told.Frame(shown)).Regions.Select(region => region.Node).SelectMany(Every))
            .First(node => node.Number("sceneRow") == row)
            .Number("event");

    // Every record a told tree holds, depth first, so a scene anywhere is found.
    static IEnumerable<Node> Every(Node node)
    {
        if (node.IsRecord)
        {
            yield return node;
        }
        foreach (var child in node.Record.Values.SelectMany(member => member.IsList ? member.List : [member]))
        {
            foreach (var found in Every(child))
            {
                yield return found;
            }
        }
    }

    static IEnumerable<Node> Records(IEnumerable<Node> nodes) => nodes.Where(node => node.Has("sceneRow"));

    /// <summary>
    /// [NATIVE-SHM-VIEW] Tally driven exactly as its reference host drives it: open, three adds, a
    /// take, an edit to "Zoë", a restyle, a clear and a refused event, every frame leased whole and
    /// read with the same assertions the C reference makes.
    /// </summary>
    [Fact]
    public void TallyDrawsItsViewsInPlaceFromTheSharedFrame()
    {
        OnWindows(Tally);
        var library = Library.Open(Tally);
        var shown = Received(library.Open(Now(), 0, "windows", Path.GetTempPath(), false));
        First(shown);
        shown = Journey(shown, library);
        Refused(shown, library);
    }

    static void First(Shown shown)
    {
        long[] kinds = [0, 2, 2, 1, 3, 3, 3, 4, 5, 3], parents = [-1, 0, 0, 0, 3, 3, 3, 0, 0, 0], actions = [-1, -1, -1, -1, 0, 1, 2, 3, -1, 4];
        Assert.Equal(10, shown.Controls.Count);
        for (var row = 0; row < shown.Controls.Count; row++)
        {
            Assert.Equal(((RowKind)kinds[row], (int)parents[row], actions[row]), (shown.Controls[row].Kind, shown.Controls[row].Parent, shown.Controls[row].Action ?? -1));
        }
        Assert.Equal("Tally", Words(shown, 1));
        Assert.Equal("0", Words(shown, 2));
        Assert.Equal("Add one", Words(shown, 5));
        Assert.Equal("Use night colours", Words(shown, 9));
        Assert.Equal((0xFBFAF7FFu, 32.0, 18.0), (shown.Controls[0].Look.Fill, shown.Controls[0].Look.Padding, shown.Controls[0].Look.Spacing));
        Assert.Equal((0x0B6E8AFFu, 72.0, 800.0), (shown.Controls[2].Look.Ink, shown.Controls[2].Look.Size, shown.Controls[2].Look.Weight));
        Assert.Equal((0x0B6E8AFFu, 10.0), (shown.Controls[5].Look.Fill, shown.Controls[5].Look.Radius));
        Assert.All(shown.Controls, row => Assert.Equal((0.0, 0.0, 0.0, 0L, 0u, 0),
            (row.Fit.Width, row.Fit.Height, row.Fit.Grow, row.Fit.Flags, row.Fit.Border, row.Hint.Length)));
        Assert.Equal((RowKind.Column, RowKind.Line, RowKind.Press, RowKind.Entry, RowKind.Gap),
            (shown.Controls[0].Kind, shown.Controls[3].Kind, shown.Controls[5].Kind, shown.Controls[7].Kind, shown.Controls[8].Kind));
    }

    // Counting, renaming and restyling, each read back from the frame the call wrote.
    static Shown Journey(Shown shown, Library library)
    {
        for (var add = 0; add < 3; add++)
        {
            shown = After(shown, library, 5, "");
        }
        Assert.Equal("3", Words(shown, 2));
        shown = After(shown, library, 4, "");
        Assert.Equal("2", Words(shown, 2));
        shown = After(shown, library, 7, "Zoë");
        Assert.Equal(("Zoë's tally", "Zoë", "2"), (Words(shown, 1), Words(shown, 7), Words(shown, 2)));
        Assert.Equal((RowKind.Entry, "Zoë"), (shown.Controls[7].Kind, shown.Controls[7].Words));
        shown = After(shown, library, 9, "");
        Assert.Equal((0x10141FFFu, 0x7AD7F0FFu), (shown.Controls[0].Look.Fill, shown.Controls[2].Look.Ink));
        Assert.Equal(("Use day colours", "2"), (Words(shown, 9), Words(shown, 2)));
        shown = After(shown, library, 6, "");
        Assert.Equal("0", Words(shown, 2));
        return shown;
    }

    // An event that names no drawn control changes nothing, and the bridge keeps every byte it held.
    static void Refused(Shown shown, Library library)
    {
        var before = library.Bridge.Usage();
        using (var call = library.Event(Now(), Touch.Control(99, "")))
        {
            Assert.NotEqual(0, call.Status);
            var kept = FrameReader.Read(call.Frame);
            Assert.Equal(("0", "Use day colours"), (Words(kept, 2), Words(kept, 9)));
        }
        Assert.Equal(before.Bytes, library.Bridge.Usage().Bytes);
    }

    [Fact]
    public async Task StorageMeasuresItsHomeThroughTheRealBoundary()
    {
        OnWindows(Built);
        var home = Directory.CreateTempSubdirectory("osprey-home-");
        try
        {
            File.WriteAllBytes(Path.Combine(home.FullName, "big.bin"), new byte[64 * 1024]);
            var library = Library.Open(Built);
            var shown = Received(library.Open(Now(), 0, "windows", home.FullName, false));
            Assert.Equal("measure-folders", Assert.Single(shown.Requests).Kind());
            shown = await Settled(library, shown);
            Assert.Empty(shown.Requests);
            Assert.NotEmpty(shown.Controls);
            Assert.Contains("big.bin", Slices.Get("measured") ?? "", StringComparison.Ordinal);
        }
        finally
        {
            home.Delete(recursive: true);
        }
    }

    // Answers every request, oldest first, dispatching each completion against the newest frame.
    static async Task<Shown> Settled(Library library, Shown shown)
    {
        var capabilities = new Capabilities(new Pickers(() => Task.FromResult<string?>(null), _ => Task.FromResult<string?>(null), _ => Task.FromResult<string?>(null)), new Store(Path.Combine(Path.GetTempPath(), "osprey-store-unused")));
        var pending = new Queue<Ask>(shown.Requests);
        for (var round = 0; round < Rounds && pending.TryDequeue(out var ask); round++)
        {
            var completion = await capabilities.Answer(ask);
            foreach (var write in completion.Writes)
            {
                Slices.Put(write.Key, write.Text);
            }
            shown = Received(library.Event(Now(), Touch.Completed(completion)));
            foreach (var next in shown.Requests)
            {
                pending.Enqueue(next);
            }
        }
        Assert.Empty(pending);
        return shown;
    }

    static Shown Received(Sealed call)
    {
        using (call)
        {
            var shown = FrameReader.Read(call.Frame);
            Assert.Empty(shown.Diagnostics);
            foreach (var (key, text) in shown.Slices)
            {
                Slices.Put(key, text);
            }
            return shown;
        }
    }

    static Shown After(Shown shown, Library library, int row, string words)
    {
        using var call = library.Event(Now(), Touch.Control(shown.Controls[row].Action ?? -1, words));
        Assert.Equal(0, call.Status);
        return Received(call);
    }

    static string Words(Shown shown, int row) => shown.Controls[row].Words;

    static long Now() => DateTimeOffset.UtcNow.ToUnixTimeSeconds();
}
