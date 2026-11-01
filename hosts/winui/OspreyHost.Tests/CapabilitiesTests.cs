using System.Text.Json;

namespace Osprey.Host.Tests;

/// <summary>[WINUI-HOST-CAPABILITIES]: each adapter does what Osprey asked and reports it; a kind this host does not answer fails by name.</summary>
public sealed class CapabilitiesTests
{
    static JsonElement Request(Action<Utf8JsonWriter> members) => JsonDocument.Parse(Json.Object(members)).RootElement.Clone();

    static Capabilities Host(string? chosen = null, List<string>? asked = null) => new(new Pickers(() => Task.FromResult(chosen), types =>
    {
        asked?.AddRange(types);
        return Task.FromResult(chosen);
    }), new Store(Path.Combine(Path.GetTempPath(), "osprey-store-unused")));

    [Fact]
    public async Task AKindThisHostDoesNotAnswerFailsByName()
    {
        foreach (var kind in Coverage.Unanswered)
        {
            var done = await Host().Answer(Request(writer => { writer.WriteString("ticket", "t"); writer.WriteString("kind", kind); }));
            Assert.False(done.Ok);
            Assert.Contains(kind, done.Error, StringComparison.Ordinal);
        }
    }

    [Fact]
    public async Task AChosenFolderComesBackWithItsPath()
    {
        var done = await Host("D:\\Media").Answer(Request(writer => { writer.WriteString("ticket", "choose"); writer.WriteString("kind", "choose-folder"); }));
        using var result = JsonDocument.Parse(done.Result);
        Assert.Equal(("choose", "D:\\Media", false), (done.Ticket, Envelopes.Text(result.RootElement, "path"), Envelopes.Flag(result.RootElement, "cancelled")));
    }

    [Fact]
    public async Task ADismissedPickerIsACancelledChoice()
    {
        var done = await Host(null).Answer(Request(writer => { writer.WriteString("ticket", "choose"); writer.WriteString("kind", "choose-folder"); }));
        using var result = JsonDocument.Parse(done.Result);
        Assert.Equal(("", true), (Envelopes.Text(result.RootElement, "path"), Envelopes.Flag(result.RootElement, "cancelled")));
    }

    [Fact]
    public async Task AChosenFileComesBackWithItsPathAndThePickerOffersOnlyTheTypesAsked()
    {
        var asked = new List<string>();
        var done = await Host("D:\\Music\\Sunroom.mp3", asked).Answer(Request(writer =>
        {
            writer.WriteString("ticket", "choose");
            writer.WriteString("kind", "choose-file");
            writer.WriteStartArray("types");
            writer.WriteStringValue("mp3");
            writer.WriteEndArray();
            writer.WriteNumber("limit", 0);
            writer.WriteString("slice", "");
        }));
        using var result = JsonDocument.Parse(done.Result);
        Assert.Equal(("choose", "D:\\Music\\Sunroom.mp3", false), (done.Ticket, Envelopes.Text(result.RootElement, "path"), Envelopes.Flag(result.RootElement, "cancelled")));
        Assert.Equal(["mp3"], asked);
        var dismissed = await Host(null).Answer(Request(writer => { writer.WriteString("ticket", "choose"); writer.WriteString("kind", "choose-file"); }));
        using var nothing = JsonDocument.Parse(dismissed.Result);
        Assert.True(Envelopes.Flag(nothing.RootElement, "cancelled"));
    }

    static JsonElement Run(string executable, long timeout, long limit, params string[] arguments) => Request(writer =>
    {
        writer.WriteString("ticket", "run");
        writer.WriteString("kind", "run");
        writer.WriteString("executable", executable);
        writer.WriteStartArray("arguments");
        foreach (var argument in arguments)
        {
            writer.WriteStringValue(argument);
        }
        writer.WriteEndArray();
        writer.WriteNumber("timeout", timeout);
        writer.WriteNumber("limit", limit);
        writer.WriteString("slice", "out");
    });

    [Fact]
    public async Task ARunWritesItsStdoutToTheSliceAndReportsItsStatus()
    {
        var shell = Path.Combine(Environment.SystemDirectory, "cmd.exe");
        var done = await Host().Answer(Run(shell, 10_000, 65_536, "/c", "echo _googlecast._tcp SRV& exit 3"));
        using var result = JsonDocument.Parse(done.Result);
        Assert.Equal((true, 3L), (done.Ok, Envelopes.Number(result.RootElement, "status")));
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
        var done = await Host().Answer(Request(writer => { writer.WriteString("ticket", "cancel:1"); writer.WriteString("kind", "cancel"); writer.WriteString("target", "walk:9"); }));
        Assert.Equal((true, "null"), (done.Ok, done.Result));
    }

    [Fact]
    public async Task RevealingAPathThatDoesNotExistFails()
    {
        var done = await Host().Answer(Request(writer => { writer.WriteString("ticket", "reveal"); writer.WriteString("kind", "reveal"); writer.WriteString("path", Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString("N"))); }));
        Assert.False(done.Ok);
        Assert.NotEmpty(done.Error);
    }

    [Fact]
    public async Task AWalkWritesTheSliceOspreyNamedAndCompletesWithNull()
    {
        var folder = Directory.CreateTempSubdirectory("osprey-walk-");
        try
        {
            File.WriteAllBytes(Path.Combine(folder.FullName, "a.bin"), new byte[4096]);
            var done = await Host().Answer(Request(writer =>
            {
                writer.WriteString("ticket", "walk:1");
                writer.WriteString("kind", "measure-folders");
                writer.WriteStartArray("folders");
                writer.WriteStringValue(folder.FullName);
                writer.WriteEndArray();
                writer.WriteNumber("limit", 100);
                writer.WriteNumber("timeout", 60_000);
                writer.WriteString("slice", "walked");
            }));
            Assert.True(done.Ok, done.Error);
            Assert.Equal("null", done.Result);
            var write = Assert.Single(done.Writes);
            Assert.Equal("walked", write.Key);
            using var slice = JsonDocument.Parse(write.Text);
            var walked = Assert.Single(Envelopes.Items(slice.RootElement, "folders", item => item));
            Assert.Equal(folder.FullName, Envelopes.Text(walked, "path"));
            var entry = Assert.Single(Envelopes.Items(walked, "entries", item => item));
            Assert.Equal("a.bin", Envelopes.Text(entry, "name"));
            Assert.True(Envelopes.Number(entry, "bytes") >= 4096);
        }
        finally
        {
            folder.Delete(recursive: true);
        }
    }
}
