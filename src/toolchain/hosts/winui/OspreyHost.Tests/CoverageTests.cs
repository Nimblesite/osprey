namespace Osprey.Host.Tests;

/// <summary>
/// [NATIVE-HOSTS-COVERAGE]: the WinUI host's column of the coverage matrix covers every row kind and
/// every capability the framework defines, so a framework change that this host does not draw,
/// answer or refuse fails here.
/// </summary>
public sealed class CoverageTests
{
    [Fact]
    public void EveryRowKindIsDrawnWithTheCodeTheFrameworkWrites()
    {
        var view = Framework.Source("view.ospml");
        var kinds = Coverage.RowKinds;
        Assert.Equal(17, kinds.Count);
        Assert.Equal(Enumerable.Range(0, 17).Select(code => (RowKind)code), kinds);
        for (var code = 0; code < kinds.Count; code++)
        {
            Assert.Contains($"export {char.ToLowerInvariant(kinds[code].ToString()[0])}{kinds[code].ToString()[1..]} = {code}", view, StringComparison.Ordinal);
        }
    }

    [Fact]
    public void EveryToldKindIsTheOneTheFrameworkWrites()
    {
        Assert.Equal([RowKind.Field, RowKind.Record, RowKind.List], Coverage.ToldKinds);
        var told = Framework.Source("told.ospml");
        Assert.Contains("export field = 33", told, StringComparison.Ordinal);
        Assert.Contains("export record = 34", told, StringComparison.Ordinal);
        Assert.Contains("export list = 35", told, StringComparison.Ordinal);
    }

    [Fact]
    public void EveryNodeKindIsDrawnOrRefusedExactlyOnce()
    {
        Assert.Equal(Framework.NodeKinds.Order(), Coverage.Drawn.Concat(Coverage.Refused).Order());
        Assert.Empty(Coverage.Drawn.Intersect(Coverage.Refused));
    }

    [Fact]
    public void EveryNodeKindIsTheOneTheFrameworkTells()
    {
        var told = Framework.Source("told.ospml");
        Assert.All(Framework.NodeKinds, kind => Assert.Contains($"\"{kind}\"", told, StringComparison.Ordinal));
        Assert.Contains("math", Framework.NodeKinds);
        Assert.DoesNotContain("typeset", Framework.NodeKinds);
    }

    [Fact]
    public void EveryRefusedKindIsShownAsAVisibleDiagnosticNamingIt() =>
        Assert.All(Coverage.Refused, kind => Assert.Contains(kind, Coverage.Refusal(kind), StringComparison.Ordinal));

    [Fact]
    public void EveryCapabilityKindIsAnsweredOrRefusedExactlyOnce()
    {
        Assert.Equal(Framework.CapabilityKinds.Order(), Coverage.Answered.Concat(Coverage.Unanswered).Order());
        Assert.Empty(Coverage.Answered.Intersect(Coverage.Unanswered));
    }

    [Fact]
    public void TheCapabilityCodesFollowTheFrameworksOwnOrder()
    {
        var line = Framework.Source("capability.ospml").Split('\n').Single(text => text.TrimStart().StartsWith("kinds = [", StringComparison.Ordinal));
        var listed = line[(line.IndexOf('[') + 1)..line.IndexOf(']')]
            .Split(',', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries)
            .Select(name => name.Trim('"'))
            .ToList();
        Assert.Equal(listed, Ask.Kinds);
        Assert.Equal(25, Ask.Kinds.Count);
    }

    [Fact]
    public void TheWireNamesAreTheOnesTheFrameworkEncodes()
    {
        var line = Framework.Source("capability.ospml").Split('\n').Single(text => text.TrimStart().StartsWith("kinds = [", StringComparison.Ordinal));
        Assert.All(Framework.CapabilityKinds, kind => Assert.Contains($"\"{kind}\"", line, StringComparison.Ordinal));
    }

    [Fact]
    public void TheConstructorAliasesAreTheWireNamesTheEncoderWrites()
    {
        Assert.Contains("read-plists", Framework.CapabilityKinds);
        Assert.DoesNotContain("read-property-lists", Framework.CapabilityKinds);
    }

    [Fact]
    public void EverySymbolHasOneFluentGlyph()
    {
        Assert.Equal(Framework.Symbols.Order(), Glyphs.ByName.Keys.Order());
        Assert.All(Glyphs.ByName.Values, glyph => Assert.True(glyph.Length == 1 && glyph[0] is >= '\uE000' and <= '\uF8FF', $"{glyph} is not one Segoe Fluent Icons glyph"));
    }

    [Fact]
    public void TheVocabularyIsReadFromTheFramework()
    {
        Assert.Contains("meter", Framework.NodeKinds);
        Assert.Contains("context-menu", Framework.NodeKinds);
        Assert.Contains("measure-folders", Framework.CapabilityKinds);
        Assert.Contains("save-file", Coverage.Answered);
        Assert.Contains("animate", Coverage.Answered);
        Assert.Contains("process-stop", Coverage.Unanswered);
        Assert.Contains("chevron", Framework.Symbols);
        Assert.DoesNotContain("nosymbol", Framework.Symbols);
    }
}
