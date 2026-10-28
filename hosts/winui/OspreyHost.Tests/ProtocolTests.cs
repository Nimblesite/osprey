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
