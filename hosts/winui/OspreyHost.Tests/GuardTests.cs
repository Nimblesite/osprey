using System.Text.RegularExpressions;
using System.Xml.Linq;

namespace Osprey.Host.Tests;

/// <summary>
/// The host's standing rules: code-only WinUI with no XAML [WINUI-HOST-NO-XAML], no UI framework
/// beyond the Windows App SDK, and no application's name, words or slices in its source
/// [NATIVE-HOSTS-NEUTRAL].
/// </summary>
public sealed partial class GuardTests
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
    public void TheRendererReadsOnlyFieldsTheFrameworkWrites()
    {
        var encoder = Framework.Source("encode.ospml");
        var reads = HostFiles()
            .Where(path => path.EndsWith(".cs", StringComparison.Ordinal) && path.Contains($"{Path.DirectorySeparatorChar}OspreyHost{Path.DirectorySeparatorChar}", StringComparison.Ordinal))
            .SelectMany(path => FieldRead().Matches(File.ReadAllText(path)).Select(match => match.Groups[1].Value))
            .Distinct()
            .ToList();
        Assert.Contains("children", reads);
        Assert.All(reads, field => Assert.Contains($"\"{field}\"", encoder, StringComparison.Ordinal));
    }

    // A node field read by name: `Text(node, "label")`, `Items(row, "rows")` and the like.
    [GeneratedRegex(@"\b(?:Text|Items|Member|Flag|Number)\((?:node|child|item|row|tab|choice|page), ""(\w+)""")]
    private static partial Regex FieldRead();

    [Theory]
    [InlineData("examples/")]
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
