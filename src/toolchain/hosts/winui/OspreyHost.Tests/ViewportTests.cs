namespace Osprey.Host.Tests;

/// <summary>
/// [UI-PROTOCOL] The frame viewport fields: this host draws neither a frame sized to the window nor
/// a scrolling frame, so a told frame that asks for one is refused whole, wherever it nests, and
/// the content-sized defaults are kept.
/// </summary>
public sealed class ViewportTests
{
    /// <summary>A framed component with <paramref name="style"/> as its style record.</summary>
    static Node Framed(Node style, Node body = default) =>
        Node.Of(new Dictionary<string, Node> { ["kind"] = Node.Of("framed"), ["style"] = style, ["body"] = body });

    /// <summary>One member of a record.</summary>
    static Node Style(string member, Node value) => Node.Of(new Dictionary<string, Node> { [member] = value });

    /// <summary>The refusal of one told node, or a message saying it was wrongly accepted.</summary>
    static string ViewportRefusal(Node node)
    {
        try
        {
            Viewports.Refuse("root", node);
        }
        catch (HostException problem)
        {
            return problem.Message;
        }
        return "accepted a frame this host cannot draw";
    }

    [Fact]
    public void AViewportSizedFrameIsRefused()
    {
        foreach (var inset in new[] { 0L, 400L })
        {
            var problem = ViewportRefusal(Framed(Style("viewportHeightInset", Node.Of(inset))));
            Assert.Contains("viewportHeightInset", problem);
            Assert.Contains("-1", problem);
        }
    }

    [Fact]
    public void AScrollingFrameIsRefused()
    {
        var problem = ViewportRefusal(Framed(Style("scrollY", Node.Of(true))));
        Assert.Contains("scrollY", problem);
        Assert.Contains("region root", problem);
    }

    [Fact]
    public void ANestedFrameWithAViewportIsRefused()
    {
        var stack = Node.Of(new Dictionary<string, Node> { ["kind"] = Node.Of("stack"), ["children"] = Node.Of(new List<Node> { Framed(Style("viewportHeightInset", Node.Of(12L))) }) });
        Assert.Contains("viewportHeightInset", ViewportRefusal(stack));
        var body = Framed(Style("scrollY", Node.Of(true)));
        Assert.Contains("scrollY", ViewportRefusal(Framed(Node.Of(new Dictionary<string, Node>()), body)));
    }

    [Fact]
    public void AMalformedViewportFieldIsRefusedNotGuessed()
    {
        foreach (var style in new[] { Style("viewportHeightInset", Node.Of("4")), Style("viewportHeightInset", Node.Of(2.5)), Style("scrollY", Node.Of(1L)), Style("scrollY", Node.Of("true")) })
        {
            Assert.Contains("where it needs", ViewportRefusal(Framed(style)));
        }
        Assert.Contains("an object style", ViewportRefusal(Framed(Node.Of("an object"))));
    }

    [Fact]
    public void OnlyAbsentOrTheContentSizedDefaultIsAccepted()
    {
        Assert.Equal([], new[] { Framed(default), Framed(Style("viewportHeightInset", Node.Of(-1L))), Framed(Style("scrollY", Node.Of(false))), Framed(Node.Of(new Dictionary<string, Node>())) }
            .Select(problem => ViewportRefusal(problem)).Where(problem => problem != "accepted a frame this host cannot draw"));
        foreach (var inset in new[] { -2L, long.MinValue })
        {
            Assert.Contains("viewportHeightInset", ViewportRefusal(Framed(Style("viewportHeightInset", Node.Of(inset)))));
        }
    }

    [Fact]
    public void AContentSizedFrameKeepsTheDefaults()
    {
        foreach (var framed in new[]
        {
            Framed(default),
            Framed(Node.Of(new Dictionary<string, Node>())),
            Framed(Style("viewportHeightInset", Node.Of(-1L))),
            Framed(Style("scrollY", Node.Of(false))),
        })
        {
            Assert.Equal("accepted a frame this host cannot draw", ViewportRefusal(framed));
        }
    }

    [Fact]
    public void AContentSizedFrameKeepsTheDefaultsExtraNestedContentSizedBodies()
    {
        var inner = Framed(Style("viewportHeightInset", Node.Of(-1L)));
        var outer = Framed(Node.Of(new Dictionary<string, Node>()), Node.Of(new List<Node> { inner, Node.Of("a label") }));
        Assert.Equal("accepted a frame this host cannot draw", ViewportRefusal(outer));
        Assert.Equal("accepted a frame this host cannot draw", ViewportRefusal(Framed(Style("viewportHeightInset", Node.Of(-1L)), Framed(Style("scrollY", Node.Of(false))))));
    }
}
