using System.Runtime.Versioning;
using System.Diagnostics;

namespace Osprey.Host.Tests;

/// <summary>[WINUI-HOST-CAPABILITIES] The application's private store: entries stay inside its folder, reads are bounded UTF-8 and writes replace an entry whole.</summary>
[SupportedOSPlatform("windows")]
public sealed class StoreTests : IDisposable
{
    readonly DirectoryInfo fixture = Directory.CreateTempSubdirectory("osprey-store-");
    readonly DirectoryInfo root;

    public StoreTests() => root = fixture.CreateSubdirectory("store");

    public void Dispose() => fixture.Delete(recursive: true);

    static Ask Asked(long code, string ticket, params AskField[] fields) => new(code, ticket, fields);

    Completion Ask(long code, params AskField[] fields) => new Store(root.FullName).Answer(Asked(code, "t", fields));

    Completion Write(string name, string text)
    {
        Slices.Put("store-test-out", text);
        return Ask(7, new AskField(1, 0, name), new AskField(2, 0, "store-test-out"));
    }

    Completion Read(string name, long limit = 4096) =>
        Ask(6, new AskField(1, 0, name), new AskField(2, limit, ""), new AskField(3, 0, "store-test-in"));

    [Fact]
    public void AWrittenEntryReadsBackIntoTheNamedSlice()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "the store pins its folders through Windows file handles");
        Assert.True(Write("scores/best.json", "[40,30,\"Zoë\"]").Ok);
        var read = Read("scores/best.json");
        Assert.True(read.Ok, read.Error);
        Assert.Equal(new SliceWrite("store-test-in", "[40,30,\"Zoë\"]"), Assert.Single(read.Writes));
        Assert.True(File.Exists(Path.Combine(root.FullName, "scores", "best.json")));
    }

    [Fact]
    public void AListingIsSortedAndAnEmptyStoreListsNothing()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "the store pins its folders through Windows file handles");
        Assert.Equal("[]", Ask(8, new AskField(1, 0, "")).Result);
        Write("b.json", "1");
        Write("a.json", "2");
        Assert.Equal("""["a.json","b.json"]""", Ask(8, new AskField(1, 0, "")).Result);
    }

    [Fact]
    public void ADeletedEntryIsGone()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "the store pins its folders through Windows file handles");
        Write("gone.txt", "x");
        Assert.True(Ask(9, new AskField(1, 0, "gone.txt")).Ok);
        Assert.Equal("no such store entry gone.txt", Read("gone.txt").Error);
    }

    [Theory]
    [InlineData("../escape.txt")]
    [InlineData("C:\\Windows\\win.ini")]
    [InlineData("\\\\server\\share\\x")]
    [InlineData("a//b")]
    [InlineData("")]
    public void ANameOutsideTheStoreIsRefused(string name)
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "the store pins its folders through Windows file handles");
        Assert.Equal($"invalid store entry {name}", Read(name).Error);
    }

    [Fact]
    public void AWriteThroughAJunctionOutsideTheStoreIsRefused()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "the store pins its folders through Windows file handles");
        var outside = fixture.CreateSubdirectory("outside");
        var link = Path.Combine(root.FullName, "link");
        var start = new ProcessStartInfo("cmd.exe")
        {
            ArgumentList = { "/c", "mklink", "/J", link, outside.FullName },
            UseShellExecute = false,
        };
        using var junction = Process.Start(start) ?? throw new InvalidOperationException("mklink did not start");
        Assert.True(junction.WaitForExit(10_000), "mklink did not exit");
        Assert.Equal(0, junction.ExitCode);
        try
        {
            var write = Write("link/secret", "private store data");
            Assert.False(write.Ok);
            Assert.Equal($"invalid store directory {link}", write.Error);
            Assert.Empty(outside.EnumerateFileSystemInfos());
        }
        finally
        {
            // A recursive delete takes the junction for a volume mount point and fails, so
            // the link goes first, on its own.
            Directory.Delete(link);
        }
    }

    [Fact]
    public void AReadPastItsLimitOrOfBrokenTextFails()
    {
        Assert.SkipUnless(OperatingSystem.IsWindows(), "the store pins its folders through Windows file handles");
        Write("big.txt", new string('x', 100));
        Assert.Equal("store entry big.txt exceeds limit 10", Read("big.txt", 10).Error);
        File.WriteAllBytes(Path.Combine(root.FullName, "broken.txt"), [0xC3, 0x28]);
        Assert.False(Read("broken.txt").Ok);
    }

    [Fact]
    public void AWriteWhoseSliceWasNeverHeldFails() =>
        Assert.Equal("store-write slice is missing", Ask(7, new AskField(1, 0, "x.txt"), new AskField(2, 0, "store-test-never-written")).Error);
}
