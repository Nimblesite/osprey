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

    /// <summary>A lifecycle phase, such as the window becoming active.</summary>
    public static string Lifecycle(long now, string phase) => Written(writer =>
    {
        writer.WriteString("kind", "lifecycle");
        writer.WriteNumber("now", now);
        writer.WriteString("phase", phase);
    });

    static string Written(Action<Utf8JsonWriter> members) => Json.Object(members);
}

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
