using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
using System.Security.Principal;
using System.Text.Json;
using Osprey.Host.Measure;

namespace Osprey.Host.Tests.Measure;

/// <summary>
/// [UI-EFFECTS-MEASURE] The Windows folder walk on a temporary tree: allocated bytes, hidden items,
/// links never followed, hard links counted once, unreadable folders, the entry limit, the
/// deadline, cancellation and the slice's shape.
/// </summary>
public sealed partial class FolderWalkTests : IDisposable
{
    readonly DirectoryInfo root = Directory.CreateTempSubdirectory("osprey-measure-");

    public void Dispose() => root.Delete(recursive: true);

    static JsonElement Walk(string folder, int limit = 1000, TimeSpan? deadline = null)
    {
        using var slice = JsonDocument.Parse(FolderWalk.Measure([folder], limit, deadline ?? TimeSpan.FromMinutes(1), CancellationToken.None));
        return Assert.Single(Envelopes.Items(slice.RootElement, "folders", item => item.Clone()));
    }

    static IReadOnlyList<JsonElement> Entries(JsonElement folder) => Envelopes.Items(folder, "entries", item => item);

    static JsonElement Entry(JsonElement folder, string name) => Entries(folder).Single(item => Envelopes.Text(item, "name") == name);

    static long Number(JsonElement item, string name) => Envelopes.Number(item, name);

    string Folder(params string[] names) => Directory.CreateDirectory(Path.Combine([root.FullName, .. names])).FullName;

    static string Write(string folder, string name, int bytes)
    {
        var path = Path.Combine(folder, name);
        File.WriteAllBytes(path, new byte[bytes]);
        return path;
    }

    static long Cluster(string path)
    {
        Assert.True(GetDiskFreeSpace(Path.GetPathRoot(path)!, out var sectors, out var bytes, out _, out _));
        return (long)sectors * bytes;
    }

    [Fact]
    public void AFolderCountsTheAllocatedBytesAndItemsOfEverythingBelowIt()
    {
        Write(root.FullName, "big.bin", 100 * 1024);
        var three = Folder("three");
        Write(three, "a.bin", 8 * 1024);
        Write(three, "b.bin", 8 * 1024);
        Write(three, "c.bin", 8 * 1024);
        var walked = Walk(root.FullName);
        var big = Number(Entry(walked, "big.bin"), "bytes");
        Assert.True(big >= 100 * 1024, $"{big} allocated bytes for a 100 KiB file");
        Assert.Equal(0, big % Cluster(root.FullName));
        Assert.Equal((3L, 0L), (Number(Entry(walked, "three"), "items"), Number(Entry(walked, "big.bin"), "items")));
        Assert.Equal(Entries(walked).Sum(item => Number(item, "bytes")), Number(walked, "bytes"));
        Assert.Equal(5, Number(walked, "items"));
    }

    [Fact]
    public void HiddenMeansTheHiddenAttributeAndALeadingDotDoesNot()
    {
        File.SetAttributes(Write(root.FullName, "secret.bin", 16 * 1024), FileAttributes.Hidden);
        Write(root.FullName, ".dotfile", 16 * 1024);
        var walked = Walk(root.FullName);
        var secret = Entry(walked, "secret.bin");
        Assert.True(Envelopes.Flag(secret, "hidden"));
        Assert.Equal(0, Number(secret, "visible"));
        Assert.True(Number(secret, "bytes") > 0);
        Assert.False(Envelopes.Flag(Entry(walked, ".dotfile"), "hidden"));
        Assert.Equal(Number(walked, "bytes") - Number(secret, "bytes"), Number(walked, "visible"));
    }

    [Fact]
    public void EverythingBelowAHiddenFolderIsOutOfSight()
    {
        var project = Folder("project");
        Write(project, "notes.bin", 8 * 1024);
        var git = Directory.CreateDirectory(Path.Combine(project, ".git"));
        Write(Folder("project", ".git", "objects"), "pack.bin", 32 * 1024);
        git.Attributes |= FileAttributes.Hidden;
        var entry = Entry(Walk(root.FullName), "project");
        Assert.Equal(4, Number(entry, "items"));
        Assert.True(Number(entry, "bytes") >= 40 * 1024);
        Assert.Equal(Cluster(root.FullName) * ((8 * 1024 + Cluster(root.FullName) - 1) / Cluster(root.FullName)), Number(entry, "visible"));
    }

    [Fact]
    public void AJunctionIsReportedAndNeverFollowed()
    {
        var target = Folder("target");
        Write(target, "data.bin", 64 * 1024);
        var link = Path.Combine(root.FullName, "link");
        using (var mklink = Process.Start(new ProcessStartInfo("cmd.exe") { ArgumentList = { "/c", "mklink", "/J", link, target }, CreateNoWindow = true, UseShellExecute = false, RedirectStandardOutput = true })!)
        {
            mklink.WaitForExit();
            Assert.Equal(0, mklink.ExitCode);
        }
        try
        {
            var walked = Walk(root.FullName);
            var junction = Entry(walked, "link");
            Assert.Equal((true, false, 0L, 0L), (Envelopes.Flag(junction, "link"), Envelopes.Flag(junction, "folder"), Number(junction, "bytes"), Number(junction, "items")));
            Assert.Equal(Number(Entry(walked, "target"), "bytes"), Number(walked, "bytes"));
        }
        finally
        {
            // A recursive delete treats a junction as a volume mount point and fails on it, so the link goes first.
            Directory.Delete(link);
        }
    }

    [Fact]
    public void AHardLinkedFileCountsOnce()
    {
        var first = Write(root.FullName, "one.bin", 64 * 1024);
        Assert.True(CreateHardLink(Path.Combine(root.FullName, "two.bin"), first, 0), $"CreateHardLinkW failed with {Marshal.GetLastPInvokeError()}");
        var walked = Walk(root.FullName);
        var (one, two) = (Number(Entry(walked, "one.bin"), "bytes"), Number(Entry(walked, "two.bin"), "bytes"));
        Assert.Equal(0, Math.Min(one, two));
        Assert.True(Math.Max(one, two) >= 64 * 1024);
        Assert.Equal(Math.Max(one, two), Number(walked, "bytes"));
        Assert.Equal(2, Number(walked, "items"));
    }

    [Fact]
    public void AFolderThatCannotBeReadIsGuardedAndCountedAsUnreadableAtAnyDepth()
    {
        var locked = new DirectoryInfo(Folder("locked"));
        var deep = new DirectoryInfo(Folder("open", "deeper"));
        Write(locked.FullName, "inside.bin", 8 * 1024);
        var deny = new FileSystemAccessRule(WindowsIdentity.GetCurrent().User!, FileSystemRights.ListDirectory, AccessControlType.Deny);
        Lock(locked, deny, add: true);
        Lock(deep, deny, add: true);
        try
        {
            Assert.SkipWhen(Readable(locked), "This process reads folders their ACLs deny: its token has the backup privilege enabled, as Git Bash's runtime enables it. Run the tests from PowerShell.");
            var walked = Walk(root.FullName);
            var entry = Entry(walked, "locked");
            Assert.Equal((true, true, 0L), (Envelopes.Flag(entry, "guarded"), Envelopes.Flag(entry, "folder"), Number(entry, "bytes")));
            Assert.False(Envelopes.Flag(Entry(walked, "open"), "guarded"));
            Assert.Equal(2, Number(walked, "unreadable"));
        }
        finally
        {
            Lock(locked, deny, add: false);
            Lock(deep, deny, add: false);
        }
    }

    static bool Readable(DirectoryInfo folder)
    {
        try
        {
            folder.GetFiles();
            return true;
        }
        catch (UnauthorizedAccessException)
        {
            return false;
        }
    }

    static void Lock(DirectoryInfo folder, FileSystemAccessRule rule, bool add)
    {
        var rules = folder.GetAccessControl();
        if (add)
        {
            rules.AddAccessRule(rule);
        }
        else
        {
            rules.RemoveAccessRule(rule);
        }
        folder.SetAccessControl(rules);
    }

    [Fact]
    public void AMissingFolderSaysSoAndListsNothing()
    {
        var walked = Walk(Path.Combine(root.FullName, "gone"));
        Assert.Equal((true, ""), (Envelopes.Flag(walked, "missing"), Envelopes.Text(walked, "error")));
        Assert.Empty(Entries(walked));
    }

    [Fact]
    public void TheEntryLimitListsFewerChildrenButCountsThemAll()
    {
        for (var index = 0; index < 5; index++)
        {
            Write(root.FullName, $"f{index}.bin", 8 * 1024);
        }
        var all = Walk(root.FullName);
        var two = Walk(root.FullName, limit: 2);
        Assert.Equal((2, true, false), (Entries(two).Count, Envelopes.Flag(two, "truncated"), Envelopes.Flag(all, "truncated")));
        Assert.Equal((5L, Number(all, "bytes")), (Number(two, "items"), Number(two, "bytes")));
    }

    [Fact]
    public void ADeadlineThatHasPassedReportsTruncated() =>
        Assert.True(Envelopes.Flag(Walk(root.FullName, deadline: TimeSpan.Zero), "truncated"));

    [Fact]
    public void ACancelledWalkThrows()
    {
        using var cancel = new CancellationTokenSource();
        cancel.Cancel();
        Assert.Throws<OperationCanceledException>(() => FolderWalk.Measure([root.FullName], 10, TimeSpan.FromMinutes(1), cancel.Token));
    }

    [Fact]
    public void ATreeDeeperThanAnyPathLimitIsWalkedWithoutRecursion()
    {
        var deepest = Folder([.. Enumerable.Repeat("level", 120)]);
        Write(deepest, "bottom.bin", 8 * 1024);
        Assert.True(deepest.Length > 260);
        Assert.Equal(120, Number(Entry(Walk(root.FullName), "level"), "items"));
    }

    [Fact]
    public void NamesKeepTheirCharactersAndTheVolumeIsReported()
    {
        Write(root.FullName, "Zoë – café.txt", 10);
        var walked = Walk(root.FullName);
        Assert.Equal(Path.Combine(root.FullName, "Zoë – café.txt"), Envelopes.Text(Entry(walked, "Zoë – café.txt"), "path"));
        var volume = Envelopes.Member(walked, "volume");
        Assert.True(Number(volume, "total") > 0 && Number(volume, "available") > 0 && Envelopes.Text(volume, "name").Length > 0);
    }

    [Fact]
    public void TheKeysComeInTheDocumentedOrder()
    {
        Write(root.FullName, "a.bin", 10);
        var walked = Walk(root.FullName);
        Assert.Equal(["path", "error", "missing", "truncated", "unreadable", "bytes", "visible", "items", "volume", "entries"], walked.EnumerateObject().Select(member => member.Name));
        Assert.Equal(["name", "total", "available"], Envelopes.Member(walked, "volume").EnumerateObject().Select(member => member.Name));
        Assert.Equal(["name", "path", "bytes", "visible", "items", "hidden", "folder", "link", "guarded"], Entries(walked)[0].EnumerateObject().Select(member => member.Name));
    }

    [LibraryImport("kernel32.dll", EntryPoint = "GetDiskFreeSpaceW", SetLastError = true, StringMarshalling = StringMarshalling.Utf16)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool GetDiskFreeSpace(string root, out uint sectorsPerCluster, out uint bytesPerSector, out uint freeClusters, out uint totalClusters);

    [LibraryImport("kernel32.dll", EntryPoint = "CreateHardLinkW", SetLastError = true, StringMarshalling = StringMarshalling.Utf16)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool CreateHardLink(string link, string existing, nint security);
}
