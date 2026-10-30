using System.Text.Json;

namespace Osprey.Host.Tests;

/// <summary>[WINUI-HOST-CAPABILITIES] The application's private store: entries stay inside its folder, reads are bounded UTF-8 and writes replace an entry whole.</summary>
public sealed class StoreTests : IDisposable
{
    readonly DirectoryInfo root = Directory.CreateTempSubdirectory("osprey-store-");

    public void Dispose() => root.Delete(recursive: true);

    Completion Ask(string kind, Action<System.Text.Json.Utf8JsonWriter> members) =>
        new Store(root.FullName).Answer("t", JsonDocument.Parse(Json.Object(writer =>
        {
            writer.WriteString("ticket", "t");
            writer.WriteString("kind", kind);
            members(writer);
        })).RootElement.Clone());

    Completion Write(string name, string text)
    {
        Slices.Put("store-test-out", text);
        return Ask("store-write", writer => { writer.WriteString("name", name); writer.WriteString("slice", "store-test-out"); });
    }

    Completion Read(string name, long limit = 4096) =>
        Ask("store-read", writer => { writer.WriteString("name", name); writer.WriteNumber("limit", limit); writer.WriteString("slice", "store-test-in"); });

    [Fact]
    public void AWrittenEntryReadsBackIntoTheNamedSlice()
    {
        Assert.True(Write("scores/best.json", "[40,30,\"Zoë\"]").Ok);
        var read = Read("scores/best.json");
        Assert.True(read.Ok, read.Error);
        Assert.Equal(new SliceWrite("store-test-in", "[40,30,\"Zoë\"]"), Assert.Single(read.Writes));
        Assert.True(File.Exists(Path.Combine(root.FullName, "scores", "best.json")));
    }

    [Fact]
    public void AListingIsSortedAndAnEmptyStoreListsNothing()
    {
        Assert.Equal("[]", Ask("store-list", writer => writer.WriteString("name", "")).Result);
        Write("b.json", "1");
        Write("a.json", "2");
        Assert.Equal("""["a.json","b.json"]""", Ask("store-list", writer => writer.WriteString("name", "")).Result);
    }

    [Fact]
    public void ADeletedEntryIsGone()
    {
        Write("gone.txt", "x");
        Assert.True(Ask("store-delete", writer => writer.WriteString("name", "gone.txt")).Ok);
        Assert.Equal("no such store entry gone.txt", Read("gone.txt").Error);
    }

    [Theory]
    [InlineData("../escape.txt")]
    [InlineData("C:\\Windows\\win.ini")]
    [InlineData("\\\\server\\share\\x")]
    [InlineData("a//b")]
    [InlineData("")]
    public void ANameOutsideTheStoreIsRefused(string name) =>
        Assert.Equal($"invalid store entry {name}", Read(name).Error);

    [Fact]
    public void AReadPastItsLimitOrOfBrokenTextFails()
    {
        Write("big.txt", new string('x', 100));
        Assert.Equal("store entry big.txt exceeds limit 10", Read("big.txt", 10).Error);
        File.WriteAllBytes(Path.Combine(root.FullName, "broken.txt"), [0xC3, 0x28]);
        Assert.False(Read("broken.txt").Ok);
    }

    [Fact]
    public void AWriteWhoseSliceWasNeverHeldFails() =>
        Assert.Equal("store-write slice is missing", Ask("store-write", writer => { writer.WriteString("name", "x.txt"); writer.WriteString("slice", "store-test-never-written"); }).Error);
}
