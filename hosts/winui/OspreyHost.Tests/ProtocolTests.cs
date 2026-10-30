using System.Text.Json;

namespace Osprey.Host.Tests;

/// <summary>[UI-PROTOCOL] and [UI-REGIONS]: envelopes decode within bounds, events encode as the framework reads them, and the region store keeps what Osprey did not resend.</summary>
public sealed class ProtocolTests
{
    const string Full = """
        {"version":1,"state":"{\"s\":1}","slices":[{"key":"walk","text":"[1,2]"}],
         "patches":[{"key":"root","revision":3,"node":{"kind":"text","text":"Hi"}}],
         "removed":["old"],"requests":[{"ticket":"t1","kind":"reveal","path":"C:\\"}],
         "diagnostics":["late event"],"theme":{"tokens":[]}}
        """;

    [Fact]
    public void AnEnvelopeDecodesEveryMember()
    {
        var envelope = Envelopes.Parse(Full);
        Assert.Equal("{\"s\":1}", envelope.State);
        Assert.Equal(new SliceWrite("walk", "[1,2]"), Assert.Single(envelope.Slices));
        var patch = Assert.Single(envelope.Patches);
        Assert.Equal(("root", 3L, "text"), (patch.Key, patch.Revision, Envelopes.Text(patch.Node, "kind")));
        Assert.Equal(["old"], envelope.Removed);
        Assert.Equal("C:\\", Envelopes.Text(Assert.Single(envelope.Requests), "path"));
        Assert.Equal(["late event"], envelope.Diagnostics);
        Assert.Equal(JsonValueKind.Object, envelope.Theme.ValueKind);
    }

    [Fact]
    public void AnotherProtocolVersionIsRefused() =>
        Assert.Contains("protocol 2", Assert.Throws<HostException>(() => Envelopes.Parse("""{"version":2,"state":""}""")).Message);

    [Fact]
    public void AMemberOfTheWrongKindIsRefusedNotGuessed()
    {
        Assert.Throws<HostException>(() => Envelopes.Parse("""{"version":1,"patches":{}}"""));
        Assert.Throws<HostException>(() => Envelopes.Parse("""{"version":1,"removed":[7]}"""));
        Assert.Throws<HostException>(() => Envelopes.Parse("""{"version":1,"patches":[{"key":"a","revision":"3"}]}"""));
    }

    [Fact]
    public void ATreeDeeperThanTheBoundIsRefused()
    {
        var deep = """{"version":1,"patches":[{"key":"root","revision":1,"node":""" + string.Concat(Enumerable.Repeat("""{"c":""", 300)) + "1" + new string('}', 300) + "}]}";
        Assert.ThrowsAny<JsonException>(() => Envelopes.Parse(deep));
    }

    [Fact]
    public void TheLaunchContextNamesWindowsAndKeepsTextReadable()
    {
        using var context = JsonDocument.Parse(Events.Context(1_700_000_000, 39_600, "C:\\Users\\Zoë", ["--elevated"]));
        var root = context.RootElement;
        Assert.Equal(("windows", "C:\\Users\\Zoë", 39_600L), (Envelopes.Text(root, "platform"), Envelopes.Text(root, "home"), Envelopes.Number(root, "offset")));
        Assert.Equal(["--elevated"], Envelopes.Items(root, "arguments", item => item.GetString()));
        Assert.Contains("Zoë", Events.Context(1, 0, "C:\\Users\\Zoë", []));
    }

    [Theory]
    [InlineData(null, JsonValueKind.Null)]
    [InlineData("D:\\Media", JsonValueKind.String)]
    [InlineData(true, JsonValueKind.True)]
    [InlineData(420L, JsonValueKind.Number)]
    public void AControlSendsItsOrdinalAndOneValue(object? value, JsonValueKind kind)
    {
        using var sent = JsonDocument.Parse(Events.Control(5, "listing", 2, 7, value));
        var root = sent.RootElement;
        Assert.Equal(("ui", "listing", 2L, 7L), (Envelopes.Text(root, "kind"), Envelopes.Text(root, "region"), Envelopes.Number(root, "revision"), Envelopes.Number(root, "ordinal")));
        Assert.Equal(kind, Envelopes.Member(root, "value").ValueKind);
    }

    [Fact]
    public void ASceneSendsItsKeysAndPointerAsObjects()
    {
        using var key = JsonDocument.Parse(Events.Control(5, "field", 3, 0, new KeyInput("left", true)));
        var pressed = Envelopes.Member(key.RootElement, "value");
        Assert.Equal(("left", true), (Envelopes.Text(pressed, "key"), Envelopes.Flag(pressed, "down")));
        using var pointer = JsonDocument.Parse(Events.Control(5, "field", 3, 0, new PointerInput(12.5, 7.25, true, false)));
        var point = Envelopes.Member(pointer.RootElement, "value");
        Assert.Equal((12.5, 7.25, true, false), (point.GetProperty("x").GetDouble(), point.GetProperty("y").GetDouble(), Envelopes.Flag(point, "down"), Envelopes.Flag(point, "shift")));
    }

    [Fact]
    public void AFrameCarriesTheSecondsSinceThePreviousOne()
    {
        using var frame = JsonDocument.Parse(Events.Frame(9, 0.016));
        Assert.Equal(("frame", 9L, 0.016), (Envelopes.Text(frame.RootElement, "kind"), Envelopes.Number(frame.RootElement, "now"), frame.RootElement.GetProperty("elapsed").GetDouble()));
    }

    [Fact]
    public void AControlCannotSendAValueTheFrameworkDoesNotRead() =>
        Assert.Throws<HostException>(() => Events.Control(5, "root", 1, 0, 1.5));

    [Fact]
    public void ACompletionCarriesItsResultOrItsError()
    {
        using var done = JsonDocument.Parse(Events.Completed(6, Completion.Done("walk:1", """{"path":"D:\\"}""")));
        using var failed = JsonDocument.Parse(Events.Completed(6, Completion.Failed("walk:2", "cancelled")));
        Assert.True(Envelopes.Flag(done.RootElement, "ok"));
        Assert.Equal("D:\\", Envelopes.Text(Envelopes.Member(done.RootElement, "result"), "path"));
        Assert.False(Envelopes.Flag(failed.RootElement, "ok"));
        Assert.Equal(("walk:2", "cancelled"), (Envelopes.Text(failed.RootElement, "ticket"), Envelopes.Text(failed.RootElement, "error")));
    }

    [Fact]
    public void ALifecyclePhaseIsSentByName() =>
        Assert.Equal("active", Envelopes.Text(JsonDocument.Parse(Events.Lifecycle(1, "active")).RootElement, "phase"));

    /// <summary>One envelope carrying a single patch node, as the framework would send it.</summary>
    static string EnvelopeOf(string node) =>
        """{"version":1,"state":"s","requests":[],"patches":[{"key":"root","revision":1,"node":NODE}]}""".Replace("NODE", node, StringComparison.Ordinal);

    /// <summary>A frame with <paramref name="style"/>, spelled as the framework writes it.</summary>
    static string Frame(string style) =>
        """{"kind":"framed","style":STYLE,"body":{"kind":"empty"}}""".Replace("STYLE", style, StringComparison.Ordinal);

    /// <summary>A frame style whose one member has <paramref name="value"/>.</summary>
    static string StyleOf(string member, string value) =>
        """{"MEMBER":VALUE}""".Replace("MEMBER", member, StringComparison.Ordinal).Replace("VALUE", value, StringComparison.Ordinal);

    /// <summary>The refusal for one patch node, or a message saying it was wrongly accepted.</summary>
    static string ViewportRefusal(string node)
    {
        try
        {
            Envelopes.Parse(EnvelopeOf(node));
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
        foreach (var inset in new[] { "0", "400" })
        {
            var problem = ViewportRefusal(Frame(StyleOf("viewportHeightInset", inset)));
            Assert.Contains("viewportHeightInset", problem);
            Assert.Contains("-1", problem);
        }
    }

    [Fact]
    public void AScrollingFrameIsRefused()
    {
        var problem = ViewportRefusal(Frame(StyleOf("scrollY", "true")));
        Assert.Contains("scrollY", problem);
        Assert.Contains("region root", problem);
    }

    [Fact]
    public void ANestedFrameWithAViewportIsRefused()
    {
        var child = ViewportRefusal("""{"kind":"stack","children":[FRAME]}""".Replace("FRAME", Frame(StyleOf("viewportHeightInset", "12")), StringComparison.Ordinal));
        Assert.Contains("viewportHeightInset", child);
        var body = ViewportRefusal("""{"kind":"framed","style":{},"body":FRAME}""".Replace("FRAME", Frame(StyleOf("scrollY", "true")), StringComparison.Ordinal));
        Assert.Contains("scrollY", body);
    }

    [Fact]
    public void AMalformedViewportFieldIsRefusedNotGuessed()
    {
        foreach (var style in new[] { StyleOf("viewportHeightInset", """ "4" """), StyleOf("viewportHeightInset", "2.5"), StyleOf("scrollY", "1"), StyleOf("scrollY", """ "true" """) })
        {
            Assert.Contains("where it needs", ViewportRefusal(Frame(style)));
        }
    }

    [Fact]
    public void OnlyAbsentOrTheContentSizedDefaultIsAccepted()
    {
        (string Node, string Wanted)[] cases =
        [
            ("""{"kind":"framed","style":null,"body":{"kind":"empty"}}""", "an object style"),
            (Frame(StyleOf("viewportHeightInset", "null")), "an integer viewportHeightInset"),
            (Frame(StyleOf("scrollY", "null")), "a boolean scrollY"),
            (Frame(StyleOf("viewportHeightInset", "-2")), "viewportHeightInset"),
            (Frame(StyleOf("viewportHeightInset", "-9223372036854775808")), "viewportHeightInset"),
        ];
        foreach (var (node, wanted) in cases)
        {
            Assert.Contains(wanted, ViewportRefusal(node));
        }
    }

    [Fact]
    public void AContentSizedFrameKeepsTheDefaults()
    {
        foreach (var node in new[] { """{"kind":"framed","body":{"kind":"empty"}}""", Frame("{}"), Frame("""{"viewportHeightInset":-1,"scrollY":false}""") })
        {
            Assert.Single(Envelopes.Parse(EnvelopeOf(node)).Patches);
        }
    }

    [Fact]
    public void TheRegionStoreReplacesPatchedRegionsAndKeepsTheRest()
    {
        var store = new RegionStore();
        store.Apply(Envelopes.Parse("""{"version":1,"patches":[{"key":"root","revision":1,"node":{"kind":"slot","key":"listing"}},{"key":"listing","revision":1,"node":{"kind":"empty"}}]}"""));
        var changed = store.Apply(Envelopes.Parse("""{"version":1,"patches":[{"key":"listing","revision":2,"node":{"kind":"rows"}}],"removed":["gone"]}"""));
        Assert.Equal(["listing", "gone"], changed);
        Assert.Equal(1, store.Find(RegionStore.Root)?.Revision);
        Assert.Equal(("rows", 2L), (Envelopes.Text(store.Find("listing")!.Node, "kind"), store.Find("listing")!.Revision));
        store.Apply(Envelopes.Parse("""{"version":1,"removed":["listing"]}"""));
        Assert.Null(store.Find("listing"));
        Assert.Single(store.All);
    }
}
