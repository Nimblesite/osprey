using System.Xml.Linq;

namespace Osprey.Host.Tests;

/// <summary>
/// The host's standing rules: code-only WinUI with no XAML [WINUI-HOST-NO-XAML], no UI framework
/// beyond the Windows App SDK, one typed host path with no JSON envelope [NATIVE-SHM-VIEW], and no
/// application's name, words or slices in its source [NATIVE-HOSTS-NEUTRAL].
/// </summary>
public sealed class GuardTests
{
    static readonly string[] Projects = ["OspreyHost", "OspreyHost.Core"];

    static IEnumerable<string> HostFiles() =>
        Projects
            .SelectMany(project => Directory.EnumerateFiles(Path.Combine(Framework.Host, project), "*", SearchOption.AllDirectories))
            .Where(path => !path.Split(Path.DirectorySeparatorChar).Any(part => part is "bin" or "obj"));

    [Fact]
    public void TheHostIsCodeOnlyWithNoXaml()
    {
        Assert.NotEmpty(HostFiles());
        Assert.DoesNotContain(HostFiles(), path => path.EndsWith(".xaml", StringComparison.OrdinalIgnoreCase));
    }

    [Fact]
    public void TheHostUsesNoUiFrameworkBeyondTheWindowsAppSdk()
    {
        string[] allowed = ["Microsoft.WindowsAppSDK.WinUI", "Microsoft.Windows.SDK.BuildTools"];
        var packages = HostFiles()
            .Where(path => path.EndsWith(".csproj", StringComparison.OrdinalIgnoreCase))
            .SelectMany(path => XDocument.Load(path).Descendants("PackageReference"))
            .Select(reference => (string?)reference.Attribute("Include") ?? "");
        Assert.All(packages, package => Assert.Contains(package, allowed));
    }

    [Fact]
    public void TheJsonHostIsGone()
    {
        // The envelope, its JSON document and the scene decoder it fed are gone; the region store and
        // the viewport check stay, reading the told rows [NATIVE-SHM-NODES] instead of decoded JSON.
        string[] gone = ["Envelope", "SceneDecoder", "osprey_app_start", "osprey_app_dispatch"];
        var sources = HostFiles().Where(path => path.EndsWith(".cs", StringComparison.Ordinal)).ToList();
        Assert.NotEmpty(sources);
        Assert.All(gone, word => Assert.DoesNotContain(sources, path => File.ReadAllText(path).Contains(word, StringComparison.Ordinal)));
        // Nothing an application sends is JSON: the frame's own readers and types hold no JSON at all.
        string[] boundary = ["Bridge.cs", "Library.cs", "Nodes.cs", "Regions.cs", "Rows.cs", "SceneRows.cs", "Touch.cs"];
        Assert.All(boundary, name => Assert.DoesNotContain("Json", File.ReadAllText(Path.Combine(Framework.Host, "OspreyHost.Core", name)), StringComparison.Ordinal));
    }

    [Fact]
    public void TheHostSpeaksTheTypedAbiAndLeasesEveryRun()
    {
        var library = File.ReadAllText(Path.Combine(Framework.Host, "OspreyHost.Core", "Library.cs"));
        Assert.Contains("osprey_app_argument", library, StringComparison.Ordinal);
        Assert.Contains("osprey_app_open", library, StringComparison.Ordinal);
        Assert.Contains("osprey_app_event", library, StringComparison.Ordinal);
        var bridge = File.ReadAllText(Path.Combine(Framework.Host, "OspreyHost.Core", "Bridge.cs"));
        Assert.Contains("ShapesRun = 0", bridge, StringComparison.Ordinal);
        Assert.Contains("AsksRun = 3", bridge, StringComparison.Ordinal);
        Assert.Contains("public static Frame Lease", bridge, StringComparison.Ordinal);
    }

    [Theory]
    [InlineData("src/osprey/examples/")]
    [InlineData("examples\\")]
    [InlineData("Osprey Storage")]
    [InlineData("OspreyStorage")]
    [InlineData("storage.dll")]
    [InlineData("flock")]
    [InlineData("painting")]
    [InlineData("\"measured\"")]
    [InlineData("\"listing\"")]
    [InlineData("\"places\"")]
    public void TheHostNamesNoApplication(string word) =>
        Assert.DoesNotContain(HostFiles(), path => File.ReadAllText(path).Contains(word, StringComparison.OrdinalIgnoreCase));
}
