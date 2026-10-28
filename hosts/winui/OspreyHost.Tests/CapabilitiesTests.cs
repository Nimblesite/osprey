using System.Text.Json;

namespace Osprey.Host.Tests;

/// <summary>[WINUI-HOST-CAPABILITIES]: each adapter does what Osprey asked and reports it; a kind this host does not answer fails by name.</summary>
public sealed class CapabilitiesTests
{
    static JsonElement Request(Action<Utf8JsonWriter> members) => JsonDocument.Parse(Json.Object(members)).RootElement.Clone();

    static Capabilities Host(string? chosen = null) => new(() => Task.FromResult(chosen));

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
