using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;

namespace Osprey.Host;

/// <summary>
/// The host's half of the protocol [NATIVE-HOSTS-TRANSPORT]: the launch context and the events
/// <c>ui::Events</c> decodes. Building them is all this does; what a control means is Osprey's.
/// </summary>
public static class Events
{
    /// <summary>The platform name this host reports [UI-CONTEXT-PLATFORM].</summary>
    public const string Platform = "windows";

    /// <summary>The seconds since the Unix epoch, the clock every event carries.</summary>
    public static long Now() => DateTimeOffset.UtcNow.ToUnixTimeSeconds();

    /// <summary>The launch context: the clock, the UTC offset in seconds, the platform, the home folder and the arguments.</summary>
    public static string Context(long now, long offset, string home, IEnumerable<string> arguments) => Written(writer =>
    {
        writer.WriteNumber("now", now);
        writer.WriteNumber("offset", offset);
        writer.WriteString("platform", Platform);
        writer.WriteString("home", home);
        writer.WriteStartArray("arguments");
        foreach (var argument in arguments)
        {
            writer.WriteStringValue(argument);
        }
        writer.WriteEndArray();
    });

    /// <summary>
    /// A control event: the binding ordinal the framework issued in its region revision, with the
    /// value the control delivered: null for a tap, a string, a boolean or a whole number.
    /// </summary>
    public static string Control(long now, string region, long revision, long ordinal, object? value) => Written(writer =>
    {
        writer.WriteString("kind", "ui");
        writer.WriteNumber("now", now);
        writer.WriteString("region", region);
        writer.WriteNumber("revision", revision);
        writer.WriteNumber("ordinal", ordinal);
        writer.WritePropertyName("value");
        Value(writer, value);
    });

    static void Value(Utf8JsonWriter writer, object? value)
    {
        switch (value)
        {
            case null: writer.WriteNullValue(); break;
            case string text: writer.WriteStringValue(text); break;
            case bool flag: writer.WriteBooleanValue(flag); break;
            case long whole: writer.WriteNumberValue(whole); break;
            case KeyInput key: Keyed(writer, key); break;
            case PointerInput pointer: Pointed(writer, pointer); break;
            case ViewInput view: Viewed(writer, view); break;
            case int whole: writer.WriteNumberValue(whole); break;
            default: throw new HostException($"A control cannot deliver a {value.GetType().Name}");
        }
    }

    /// <summary>A completion delivered under its ticket: the raw JSON result when it succeeded, or the error.</summary>
    public static string Completed(long now, Completion completion) => Written(writer =>
    {
        writer.WriteString("kind", "completed");
        writer.WriteNumber("now", now);
        writer.WriteString("ticket", completion.Ticket);
        writer.WriteBoolean("ok", completion.Ok);
        if (completion.Ok)
        {
            writer.WritePropertyName("result");
            writer.WriteRawValue(completion.Result);
        }
        else
        {
            writer.WriteString("error", completion.Error);
        }
    });

    // [UI-SCENE] A scene's key going down or up, by the name the framework reads.
    static void Keyed(Utf8JsonWriter writer, KeyInput key)
    {
        writer.WriteStartObject();
        writer.WriteString("key", key.Key);
        writer.WriteBoolean("down", key.Down);
        writer.WriteEndObject();
    }

    // [UI-SCENE] The pointer in scene units, pressed or not, with Shift held or not.
    static void Pointed(Utf8JsonWriter writer, PointerInput pointer)
    {
        writer.WriteStartObject();
        writer.WriteNumber("x", pointer.X);
        writer.WriteNumber("y", pointer.Y);
        writer.WriteBoolean("down", pointer.Down);
        writer.WriteBoolean("shift", pointer.Shift);
        writer.WriteEndObject();
    }

    // [UI-SCENE] The scene's region in scene units.
    static void Viewed(Utf8JsonWriter writer, ViewInput view)
    {
        writer.WriteStartObject();
        writer.WriteNumber("width", view.Width);
        writer.WriteNumber("height", view.Height);
        writer.WriteEndObject();
    }

    /// <summary>One display frame [WINUI-HOST-FRAME]: the seconds since the previous one.</summary>
    public static string Frame(long now, double elapsed) => Written(writer =>
    {
        writer.WriteString("kind", "frame");
        writer.WriteNumber("now", now);
        writer.WriteNumber("elapsed", elapsed);
    });

    /// <summary>A lifecycle phase, such as the window becoming active.</summary>
    public static string Lifecycle(long now, string phase) => Written(writer =>
    {
        writer.WriteString("kind", "lifecycle");
        writer.WriteNumber("now", now);
        writer.WriteString("phase", phase);
    });

    static string Written(Action<Utf8JsonWriter> members) => Json.Object(members);
}

/// <summary>A key a scene took, by the framework's name for it, going down or up [UI-SCENE].</summary>
public sealed record KeyInput(string Key, bool Down);

/// <summary>The pointer over a scene, in scene units, pressed or not, with Shift held or not [UI-SCENE].</summary>
public sealed record PointerInput(double X, double Y, bool Down, bool Shift);

/// <summary>A scene's region in scene units, sent when it first shows and whenever it changes size [UI-SCENE].</summary>
public sealed record ViewInput(double Width, double Height);

/// <summary>JSON the host writes: escaped only where JSON requires, so text reaches Osprey as UTF-8.</summary>
public static class Json
{
    static readonly JsonWriterOptions Relaxed = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    /// <summary>One JSON array of <paramref name="items"/>, in order.</summary>
    public static string Array(IEnumerable<string> items)
    {
        using var buffer = new MemoryStream();
        using (var writer = new Utf8JsonWriter(buffer, Relaxed))
        {
            writer.WriteStartArray();
            foreach (var item in items)
            {
                writer.WriteStringValue(item);
            }
            writer.WriteEndArray();
        }
        return Encoding.UTF8.GetString(buffer.ToArray());
    }

    /// <summary>One JSON object holding the members <paramref name="members"/> writes.</summary>
    public static string Object(Action<Utf8JsonWriter> members)
    {
        using var buffer = new MemoryStream();
        using (var writer = new Utf8JsonWriter(buffer, Relaxed))
        {
            writer.WriteStartObject();
            members(writer);
            writer.WriteEndObject();
        }
        return Encoding.UTF8.GetString(buffer.ToArray());
    }
}
