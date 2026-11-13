namespace Osprey.Host;

/// <summary>One host event as the typed call <c>osprey_app_event</c> takes it [NATIVE-SHM-EVENT]: a kind, the control it names, two texts, two numbers and two flags. Nothing is encoded; what a control means is the application's, through the binding its number names.</summary>
public sealed record Touch(long Kind, long Ordinal, string Words, string Detail, double X, double Y, bool Down, bool Shift)
{
    static Touch Of(long kind) => new(kind, 0, "", "", 0, 0, false, false);

    /// <summary>The control numbered <paramref name="ordinal"/> delivering <paramref name="words"/>: empty for a tap, <c>true</c>/<c>false</c> for a switch, a key for a picker, text for a field, a whole number for a splitter or caret.</summary>
    public static Touch Control(long ordinal, string words) => Of(TouchKind.Control) with { Ordinal = ordinal, Words = words };

    /// <summary>
    /// The words a control's value carries [UI-EVENTS]: nothing for a tap, the text for a field,
    /// <c>true</c>/<c>false</c> for a switch or disclosure, the chosen key for a picker, and a whole
    /// number in decimal for a splitter's width or an editor's caret. Anything else is refused.
    /// </summary>
    public static string WordsOf(object? value) => value switch
    {
        null => "",
        string text => text,
        bool flag => flag ? "true" : "false",
        long whole => whole.ToString(System.Globalization.CultureInfo.InvariantCulture),
        int whole => whole.ToString(System.Globalization.CultureInfo.InvariantCulture),
        _ => throw new HostException($"A control cannot deliver a {value.GetType().Name}"),
    };

    /// <summary>A key going down or up over the scene numbered <paramref name="ordinal"/>.</summary>
    public static Touch Key(long ordinal, string name, bool down) => Of(TouchKind.Key) with { Ordinal = ordinal, Words = name, Down = down };

    /// <summary>The pointer over the scene numbered <paramref name="ordinal"/>, in scene units.</summary>
    public static Touch Pointer(long ordinal, double x, double y, bool down, bool shift) =>
        Of(TouchKind.Pointer) with { Ordinal = ordinal, X = x, Y = y, Down = down, Shift = shift };

    /// <summary>The scene numbered <paramref name="ordinal"/> measured in scene units.</summary>
    public static Touch Viewed(long ordinal, double width, double height) => Of(TouchKind.Viewed) with { Ordinal = ordinal, X = width, Y = height };

    /// <summary>The capability under <paramref name="ticket"/> finished: its result text, or the reason it failed.</summary>
    public static Touch Completed(Completion completion) =>
        Of(TouchKind.Completed) with
        {
            Words = completion.Ticket,
            Detail = completion.Ok ? completion.Result : completion.Error,
            Down = completion.Ok,
        };

    /// <summary>A lifecycle phase, such as <c>active</c>, <c>closed</c> or <c>open:&lt;path&gt;</c>.</summary>
    public static Touch Phase(string name) => Of(TouchKind.Phase) with { Words = name };

    /// <summary>One display frame, <paramref name="elapsed"/> seconds after the last.</summary>
    public static Touch Frame(double elapsed) => Of(TouchKind.Frame) with { X = elapsed };

    /// <summary>The system appearance changed.</summary>
    public static Touch Appearance(bool dark) => Of(TouchKind.Appearance) with { Down = dark };
}

/// <summary>The event kinds <c>ui::Hosting</c> declares [NATIVE-SHM-EVENT].</summary>
public static class TouchKind
{
    /// <summary>A control delivering its ordinal and words.</summary>
    public const long Control = 0;

    /// <summary>A key going down or up.</summary>
    public const long Key = 1;

    /// <summary>The pointer, pressed or not.</summary>
    public const long Pointer = 2;

    /// <summary>A scene's region in scene units.</summary>
    public const long Viewed = 3;

    /// <summary>A scene point.</summary>
    public const long Point = 4;

    /// <summary>A scene stroke.</summary>
    public const long Stroke = 5;

    /// <summary>An enclosure.</summary>
    public const long Enclose = 6;

    /// <summary>A lasso.</summary>
    public const long Lasso = 7;

    /// <summary>A capability completed under its ticket.</summary>
    public const long Completed = 8;

    /// <summary>A lifecycle phase.</summary>
    public const long Phase = 9;

    /// <summary>One display frame.</summary>
    public const long Frame = 10;

    /// <summary>The system appearance changed.</summary>
    public const long Appearance = 11;
}
