using System.Runtime.InteropServices;

namespace Osprey.Host;

/// <summary>
/// The Osprey application DLL [WINUI-HOST-DLL]: its entry, its exports, the import slots it calls
/// back through [WINUI-HOST-IMPORTS] and the shared-memory bridge it carries
/// [NATIVE-SHM-OWNERSHIP]. Every call is made on the UI thread [WINUI-HOST-THREAD], and a
/// returned string is copied before the next call, because it belongs to Osprey. The frame each
/// export writes is sealed here: committed when the call succeeded, discarded when it failed, so
/// no half-written view is ever published [NATIVE-SHM-VIEW].
/// </summary>
public sealed unsafe class Library
{
    const string Entry = "osprey_main";
    const string StartExport = "osprey_app_start";
    const string DispatchExport = "osprey_app_dispatch";
    const string EventExport = "osprey_app_event";
    const string BinderPrefix = "osprey_import_";
    const int Unbound = -2;

    readonly delegate* unmanaged<byte*, byte*> started;
    readonly delegate* unmanaged<byte*, byte*, byte*> dispatched;
    readonly delegate* unmanaged<long> began;
    readonly delegate* unmanaged<long, byte*, long> delivered;

    Library(Bridge bridge, bool shared, nint start, nint dispatch, nint sent)
    {
        Bridge = bridge;
        Shared = shared;
        if (shared)
        {
            began = (delegate* unmanaged<long>)(void*)start;
            delivered = (delegate* unmanaged<long, byte*, long>)(void*)sent;
            return;
        }
        started = (delegate* unmanaged<byte*, byte*>)(void*)start;
        dispatched = (delegate* unmanaged<byte*, byte*, byte*>)(void*)dispatch;
    }

    /// <summary>The shared-memory bridge this DLL carries, opened once before its entry ran.</summary>
    public Bridge Bridge { get; }

    /// <summary>Whether this application draws the shared view [NATIVE-SHM-VIEW]: it exports <c>osprey_app_event</c> and no <c>osprey_app_dispatch</c>.</summary>
    public bool Shared { get; }

    /// <summary>The streamed shapes of the frame the last envelope call wrote, leased in place until the next call replaces them.</summary>
    public Run<NativeShape>? Shapes { get; private set; }

    /// <summary>The imports this host answers, by the name an application declares them with.</summary>
    public static IReadOnlyDictionary<string, nint> Imports { get; } = new Dictionary<string, nint>
    {
        ["native_slice"] = (nint)(delegate* unmanaged<byte*, byte*>)&Slices.Answer,
    };

    /// <summary>Loads the application, opens its bridge, fills every slot it declares and runs its entry once.</summary>
    public static Library Open(string path)
    {
        var handle = NativeLibrary.Load(Path.GetFullPath(path));
        foreach (var (name, function) in Imports)
        {
            Bind(handle, name, function);
        }
        var bridge = Bridge.Open(handle);
        Enter(Export(handle, Entry), path);
        var shared = NativeLibrary.TryGetExport(handle, EventExport, out var sent) && !NativeLibrary.TryGetExport(handle, DispatchExport, out _);
        return new Library(bridge, shared, Export(handle, StartExport), shared ? 0 : Export(handle, DispatchExport), shared ? sent : 0);
    }

    static void Bind(nint handle, string name, nint function)
    {
        if (NativeLibrary.TryGetExport(handle, BinderPrefix + name, out var binder))
        {
            ((delegate* unmanaged<nint, void>)binder)(function);
        }
    }

    static void Enter(nint entry, string path)
    {
        var status = ((delegate* unmanaged<int>)entry)();
        if (status == Unbound)
        {
            throw new HostException($"{path} imports a function this host does not answer; it answers {string.Join(", ", Imports.Keys)}");
        }
        if (status != 0)
        {
            throw new HostException($"{path}: {Entry} returned {status}");
        }
    }

    static nint Export(nint handle, string name) =>
        NativeLibrary.TryGetExport(handle, name, out var address)
            ? address
            : throw new HostException($"The application exports no {name}; build it with `osprey <project> --compile --target=windows-x64 -o app.dll`");

    /// <summary>The start envelope for the launch <paramref name="context"/>, with the frame the call wrote sealed and its shapes leased.</summary>
    public string Start(string context)
    {
        Requires(shared: false);
        using var source = new Utf8Text(context);
        return Sealed(() => Copied(started(source.Pointer)));
    }

    /// <summary>The next envelope for the opaque <paramref name="state"/> and one <paramref name="eventText"/>, with the frame the call wrote sealed and its shapes leased.</summary>
    public string Dispatch(string state, string eventText)
    {
        Requires(shared: false);
        using var saved = new Utf8Text(state);
        using var message = new Utf8Text(eventText);
        return Sealed(() => Copied(dispatched(saved.Pointer, message.Pointer)));
    }

    /// <summary>Draws the first shared view; zero when it was written [NATIVE-SHM-VIEW].</summary>
    public long StartView()
    {
        Requires(shared: true);
        var status = began();
        Sealed(status);
        return status;
    }

    /// <summary>Delivers the control numbered <paramref name="ordinal"/> with its edited <paramref name="words"/> and draws the next view; nonzero when the application refused, which changes nothing.</summary>
    public long EventView(long ordinal, string words)
    {
        Requires(shared: true);
        using var text = new Utf8Text(words);
        var status = delivered(ordinal, text.Pointer);
        Sealed(status);
        return status;
    }

    // One call path per application shape: envelopes for scene applications, events for shared views.
    void Requires(bool shared)
    {
        if (shared != Shared)
        {
            throw new HostException(shared
                ? "A shared-view application takes events, not envelopes"
                : "An envelope application takes envelopes, not events");
        }
    }

    // Seals the frame the call wrote: committed when it answered, discarded when it failed.
    string Sealed(Func<string> call)
    {
        string envelope;
        try
        {
            envelope = call();
        }
        catch
        {
            Bridge.Discard();
            throw;
        }
        Bridge.Commit();
        Shapes = Leased(Shapes);
        return envelope;
    }

    void Sealed(long status)
    {
        if (status != 0)
        {
            Bridge.Discard();
            return;
        }
        Bridge.Commit();
    }

    // The new frame's shapes are leased before the previous lease is released, so the shapes being drawn stay pinned.
    Run<NativeShape> Leased(Run<NativeShape>? previous)
    {
        var next = Bridge.Take<NativeShape>(Bridge.ShapesRun);
        previous?.Dispose();
        return next;
    }

    static string Copied(byte* value) =>
        value is null
            ? throw new HostException("The Osprey application returned no envelope")
            : Marshal.PtrToStringUTF8((nint)value) ?? "";
}

/// <summary>A protocol or boundary failure, shown to the person rather than hidden.</summary>
public sealed class HostException(string message) : Exception(message);
