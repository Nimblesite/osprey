namespace Osprey.Host.Tests;

/// <summary>[NATIVE-SHM-EVENT] The typed events <c>osprey_app_event</c> takes: a kind, the control it names, two texts, two numbers and two flags, with nothing encoded in between.</summary>
public sealed class TouchTests
{
    [Fact]
    public void AControlCarriesItsKindOrdinalAndWords()
    {
        var touch = Touch.Control(7, "true");
        Assert.Equal((TouchKind.Control, 7L, "true", "", 0.0, 0.0, false, false), (touch.Kind, touch.Ordinal, touch.Words, touch.Detail, touch.X, touch.Y, touch.Down, touch.Shift));
    }

    [Fact]
    public void AFieldsLinesEndInNewlinesAsOnEveryHost()
    {
        // WinUI's text box writes a line break as \r; the application reads Return as a trailing \n everywhere.
        foreach (var (typed, delivered) in new[] { ("Fix it\r", "Fix it\n"), ("one\r\ntwo\r", "one\ntwo\n"), ("a\rb\nc", "a\nb\nc"), ("\r\r", "\n\n"), ("plain", "plain"), ("", "") })
        {
            Assert.Equal(delivered, Touch.Control(1, typed).Words);
        }
    }

    [Fact]
    public void AKeyCarriesItsOrdinalNameAndDirection()
    {
        Assert.Equal((TouchKind.Key, 3L, "left", true), (Touch.Key(3, "left", true).Kind, Touch.Key(3, "left", true).Ordinal, Touch.Key(3, "left", true).Words, Touch.Key(3, "left", true).Down));
        Assert.False(Touch.Key(3, "left", false).Down);
    }

    [Fact]
    public void APointerCarriesItsScenePositionAndFlags()
    {
        var touch = Touch.Pointer(2, 12.5, 7.25, true, true);
        Assert.Equal((TouchKind.Pointer, 2L, 12.5, 7.25, true, true), (touch.Kind, touch.Ordinal, touch.X, touch.Y, touch.Down, touch.Shift));
    }

    [Fact]
    public void AViewedSceneCarriesItsSizeInItsTwoNumbers()
    {
        var touch = Touch.Viewed(5, 640, 480);
        Assert.Equal((TouchKind.Viewed, 5L, 640.0, 480.0), (touch.Kind, touch.Ordinal, touch.X, touch.Y));
    }

    [Fact]
    public void ACompletionCarriesItsResultOrItsReason()
    {
        var done = Touch.Completed(Completion.Done("walk:1", """{"status":0}"""));
        Assert.Equal((TouchKind.Completed, "walk:1", true, """{"status":0}"""), (done.Kind, done.Words, done.Down, done.Detail));
        var failed = Touch.Completed(Completion.Failed("walk:2", "cancelled"));
        Assert.Equal(("walk:2", false, "cancelled"), (failed.Words, failed.Down, failed.Detail));
    }

    [Fact]
    public void APhaseFrameAndAppearanceCarryOnlyTheirOwnField()
    {
        Assert.Equal(("active", TouchKind.Phase), (Touch.Phase("active").Words, Touch.Phase("active").Kind));
        Assert.Equal((TouchKind.Frame, 0.016), (Touch.Frame(0.016).Kind, Touch.Frame(0.016).X));
        Assert.Equal((TouchKind.Appearance, true), (Touch.Appearance(true).Kind, Touch.Appearance(true).Down));
        Assert.False(Touch.Appearance(false).Down);
    }

    [Fact]
    public void EveryKindTheApplicationDeclaresHasItsNumber()
    {
        Assert.Equal(0, TouchKind.Control);
        Assert.Equal(1, TouchKind.Key);
        Assert.Equal(2, TouchKind.Pointer);
        Assert.Equal(3, TouchKind.Viewed);
        Assert.Equal(4, TouchKind.Point);
        Assert.Equal(5, TouchKind.Stroke);
        Assert.Equal(6, TouchKind.Enclose);
        Assert.Equal(7, TouchKind.Lasso);
        Assert.Equal(8, TouchKind.Completed);
        Assert.Equal(9, TouchKind.Phase);
        Assert.Equal(10, TouchKind.Frame);
        Assert.Equal(11, TouchKind.Appearance);
    }

    [Fact]
    public void ASliceWriteKeepsItsKeyAndTextAndADiagnosticItsReason()
    {
        var write = new Ask(Ask.SliceWrite, "walk", [new AskField(1, 0, "[1,2]")]);
        Assert.Equal(Ask.SliceWrite, write.Code);
        Assert.Equal("", write.Kind());
        Assert.Equal("[1,2]", write.Fields[0].Text);
        var refused = new Ask(Ask.Diagnostic, "an event no control names", []);
        Assert.Equal(Ask.Diagnostic, refused.Code);
        Assert.Equal("an event no control names", refused.Ticket);
    }

    // [WINUI-HOST-THREAD] An HTTP answer that lands after the self-test's Exit, or after a person closed the
    // window, must not reach the application: WinUI is tearing its controls down, and touching them crashed
    // the host with 0xC000027B in CoreMessagingXP.
    [Fact]
    public void AnEventArrivingAfterTheApplicationEndedIsDropped()
    {
        var lifetime = new Lifetime();
        var delivered = new List<string>();
        Assert.False(lifetime.Ended);
        Assert.True(lifetime.Deliver(() => delivered.Add("while running")));
        Assert.True(lifetime.Deliver(() => delivered.Add("still running")));
        lifetime.End();
        Assert.True(lifetime.Ended);
        Assert.False(lifetime.Deliver(() => delivered.Add("late answer")));
        lifetime.End();
        Assert.True(lifetime.Ended);
        Assert.False(lifetime.Deliver(() => delivered.Add("later frame")));
        Assert.Equal(["while running", "still running"], delivered);
    }
}
