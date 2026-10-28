namespace Osprey.Host.Tests;

/// <summary>
/// [NATIVE-HOSTS-COVERAGE]: the WinUI host's column of the coverage matrix covers every node,
/// capability and symbol the framework defines, so a framework change that this host does not
/// draw, answer or refuse fails here.
/// </summary>
public sealed class CoverageTests
{
    [Fact]
    public void EveryNodeKindIsDrawnOrRefusedExactlyOnce()
    {
        Assert.Equal(Framework.NodeKinds.Order(), Coverage.Drawn.Concat(Coverage.Refused).Order());
        Assert.Empty(Coverage.Drawn.Intersect(Coverage.Refused));
    }

    [Fact]
    public void EveryCapabilityKindIsAnsweredOrRefusedExactlyOnce()
    {
        Assert.Equal(Framework.CapabilityKinds.Order(), Coverage.Answered.Concat(Coverage.Unanswered).Order());
        Assert.Empty(Coverage.Answered.Intersect(Coverage.Unanswered));
    }

    [Fact]
    public void TheWireNamesAreTheOnesTheFrameworkEncodes()
    {
        var nodes = Framework.Source("encode.ospml");
        var requests = Framework.Source("capability.ospml");
        Assert.All(Framework.NodeKinds, kind => Assert.Contains($"\"{kind}\"", nodes, StringComparison.Ordinal));
        Assert.All(Framework.CapabilityKinds, kind => Assert.Contains($"\"kind\" \"{kind}\"", requests, StringComparison.Ordinal));
    }

    [Fact]
    public void EverySymbolHasOneFluentGlyph()
    {
        Assert.Equal(Framework.Symbols.Order(), Glyphs.ByName.Keys.Order());
        Assert.All(Glyphs.ByName.Values, glyph => Assert.True(glyph.Length == 1 && glyph[0] is >= '' and <= '', $"{glyph} is not one Segoe Fluent Icons glyph"));
    }

    [Fact]
    public void TheVocabularyIsReadFromTheFramework()
    {
        Assert.Contains("meter", Framework.NodeKinds);
        Assert.Contains("context-menu", Framework.NodeKinds);
        Assert.Contains("measure-folders", Framework.CapabilityKinds);
        Assert.Contains("chevron", Framework.Symbols);
        Assert.DoesNotContain("nosymbol", Framework.Symbols);
    }
}
