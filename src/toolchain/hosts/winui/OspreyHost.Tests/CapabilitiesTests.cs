using System.Runtime.Versioning;
namespace Osprey.Host.Tests;

/// <summary>[WINUI-HOST-CAPABILITIES]: each adapter does what Osprey asked and reports it; a kind this host does not answer fails by name, and a chooser answers with the bare path.</summary>
[SupportedOSPlatform("windows")]
public sealed class CapabilitiesTests
{
    static Ask Asked(long code, string ticket, params AskField[] fields) => new(code, ticket, fields);

    static AskField Text(long field, string text) => new(field, 0, text);

    static AskField Number(long field, long number) => new(field, number, "");

    static Capabilities Host(string? chosen = null, List<string>? asked = null, string? saved = null) => new(
        new Pickers(
            () => Task.FromResult(chosen),
            types =>
            {
                asked?.AddRange(types);
                return Task.FromResult(chosen);
            },
            _ => Task.FromResult(saved)),
        new Store(Path.Combine(Path.GetTempPath(), "osprey-store-unused")));

    [Fact]
    public async Task AKindThisHostDoesNotAnswerFailsByName()
    {
        foreach (var (kind, at) in Framework.CapabilityKinds.Select((kind, at) => (kind, at)))
        {
            if (Coverage.Answered.Contains(kind))
            {
                continue;
            }
            var done = await Host().Answer(Asked(at + 1, "t"));
            Assert.False(done.Ok);
            Assert.Contains(kind, done.Error, StringComparison.Ordinal);
        }
    }

    [Fact]
    public async Task AChosenFolderComesBackWithItsPath()
    {
        var done = await Host("D:\\Media").Answer(Asked(14, "choose"));
        Assert.Equal(("choose", true, "D:\\Media"), (done.Ticket, done.Ok, done.Result));
    }

    [Fact]
    public async Task AChosenFolderComesBackWithItsPathExtraUnreadFieldsChangeNothing()
    {
        var done = await Host("E:\\Zoë's Folder").Answer(Asked(14, "choose", Number(2, 5), Text(3, "ignored")));
        Assert.Equal(("choose", true, "E:\\Zoë's Folder"), (done.Ticket, done.Ok, done.Result));
        Assert.Empty(done.Writes);
    }

    [Fact]
    public async Task ADismissedPickerIsACancelledChoice()
    {
        var done = await Host(null).Answer(Asked(14, "choose"));
        Assert.Equal(("", true), (done.Result, done.Ok));
    }

    [Fact]
    public async Task ADismissedPickerIsACancelledChoiceExtraTheFilePickerStillOfferedItsTypes()
    {
        var asked = new List<string>();
        var done = await Host(null, asked).Answer(Asked(13, "choose", Text(1, "mp3"), Text(1, "wav")));
        Assert.Equal(["mp3", "wav"], asked);
        Assert.Equal((true, ""), (done.Ok, done.Result));
    }

    [Fact]
    public async Task AChosenFileComesBackWithItsPathAndThePickerOffersOnlyTheTypesAsked()
    {
        var asked = new List<string>();
        var done = await Host("D:\\Music\\Sunroom.mp3", asked).Answer(Asked(13, "choose", Text(1, "mp3"), Number(2, 0), Text(3, "")));
        Assert.Equal(("choose", "D:\\Music\\Sunroom.mp3"), (done.Ticket, done.Result));
        Assert.Equal(["mp3"], asked);
        var dismissed = await Host(null).Answer(Asked(13, "choose"));
        Assert.Equal("", dismissed.Result);
        Assert.True(dismissed.Ok);
    }

    [Fact]
    public async Task AChosenFileComesBackWithItsPathAndThePickerOffersOnlyTheTypesAskedExtraSeveralTypesInOrder()
    {
        var asked = new List<string>();
        var done = await Host("D:\\Music\\Both.flac", asked).Answer(Asked(13, "choose", Text(1, "mp3"), Text(1, "wav"), Text(1, "flac"), Number(2, 3)));
        Assert.Equal((true, "D:\\Music\\Both.flac"), (done.Ok, done.Result));
        Assert.Equal(["mp3", "wav", "flac"], asked);
    }

    [Fact]
    public async Task ASaveWritesTheStagedTextAndCompletesWithTheChosenPath()
    {
        var target = Path.Combine(Path.GetTempPath(), $"osprey-save-{Guid.NewGuid():N}.txt");
        try
        {
            Slices.Put("staged", "the saved text");
            var done = await Host(saved: target).Answer(Asked(15, "save", Text(1, "note.txt"), Text(2, "staged")));
            Assert.Equal((true, target), (done.Ok, done.Result));
            Assert.Equal("the saved text", File.ReadAllText(target));
            var dismissed = await Host(null).Answer(Asked(15, "save", Text(1, "note.txt"), Text(2, "staged")));
            Assert.Equal((true, ""), (dismissed.Ok, dismissed.Result));
        }
        finally
        {
            File.Delete(target);
        }
    }

    [Fact]
    public async Task ASaveWhoseSliceWasNeverHeldFails()
    {
        var done = await Host(saved: Path.GetTempPath()).Answer(Asked(15, "save", Text(1, "x.txt"), Text(2, "never-written")));
        Assert.Equal((false, "save-file slice is missing"), (done.Ok, done.Error));
    }

    static Ask Run(string executable, long timeout, long limit, params string[] arguments)
    {
        var fields = new List<AskField> { Text(1, executable) };
        fields.AddRange(arguments.Select(argument => Text(2, argument)));
        fields.Add(Number(3, timeout));
        fields.Add(Number(4, limit));
        fields.Add(Number(5, 0));
        fields.Add(Text(6, "out"));
        return Asked(1, "run", [.. fields]);
    }

    [Fact]
    public async Task ARunWritesItsStdoutToTheSliceAndReportsItsStatus()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "run starts a Windows executable");
        var shell = Path.Combine(Environment.SystemDirectory, "cmd.exe");
        var done = await Host().Answer(Run(shell, 10_000, 65_536, "/c", "echo _googlecast._tcp SRV& exit 3"));
        var result = System.Text.Json.JsonDocument.Parse(done.Result);
        Assert.Equal((true, 3L), (done.Ok, Docs.Number(result.RootElement, "status")));
        Assert.Equal("out", done.Writes[0].Key);
        Assert.StartsWith("_googlecast._tcp SRV", done.Writes[0].Text, StringComparison.Ordinal);
        var relative = await Host().Answer(Run("cmd.exe", 10_000, 65_536));
        Assert.Equal((false, "Executable must be an absolute path"), (relative.Ok, relative.Error));
        var late = await Host().Answer(Run(shell, 200, 65_536, "/c", "ping -n 30 127.0.0.1"));
        Assert.Contains("deadline", late.Error, StringComparison.Ordinal);
        var loud = await Host().Answer(Run(shell, 10_000, 4, "/c", "echo more than four bytes"));
        Assert.Equal((false, "The output passed its limit"), (loud.Ok, loud.Error));
    }

    [Fact]
    public async Task CancellingAWalkThatIsNotRunningChangesNothing()
    {
        var done = await Host().Answer(Asked(19, "cancel:1", Text(1, "walk:9")));
        Assert.Equal((true, "null"), (done.Ok, done.Result));
    }

    [Fact]
    public async Task RevealingAPathThatDoesNotExistFails()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "reveal opens Windows File Explorer");
        var done = await Host().Answer(Asked(17, "reveal", Text(1, Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString("N")))));
        Assert.False(done.Ok);
        Assert.NotEmpty(done.Error);
    }

    [Fact]
    public async Task AWalkWritesTheSliceOspreyNamedAndCompletesWithNull()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "the folder walk reads Windows file ids and volumes");
        var folder = Directory.CreateTempSubdirectory("osprey-walk-");
        try
        {
            File.WriteAllBytes(Path.Combine(folder.FullName, "a.bin"), new byte[4096]);
            var done = await Host().Answer(Asked(16, "walk:1", Text(1, folder.FullName), Number(2, 100), Number(3, 60_000), Text(4, "walked")));
            Assert.True(done.Ok, done.Error);
            Assert.Equal("null", done.Result);
            var write = Assert.Single(done.Writes);
            Assert.Equal("walked", write.Key);
            using var slice = System.Text.Json.JsonDocument.Parse(write.Text);
            var walked = Assert.Single(Docs.Items(slice.RootElement, "folders", item => item));
            Assert.Equal(folder.FullName, Docs.Text(walked, "path"));
            var entry = Assert.Single(Docs.Items(walked, "entries", item => item));
            Assert.Equal("a.bin", Docs.Text(entry, "name"));
            Assert.True(Docs.Number(entry, "bytes") >= 4096);
        }
        finally
        {
            folder.Delete(recursive: true);
        }
    }
}
