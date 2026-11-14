using System.Runtime.InteropServices;

namespace Osprey.Host;

/// <summary>
/// The Osprey application DLL [WINUI-HOST-DLL]: its entry, its three exports, the import slots it
/// calls back through [WINUI-HOST-IMPORTS] and the shared-memory bridge it carries
/// [NATIVE-SHM-OWNERSHIP]. Every call is made on the UI thread [WINUI-HOST-THREAD]. Every call
/// writes one whole frame, and the host commits it whether the call answered or refused; the frame
/// is then leased whole and checked [NATIVE-SHM-VIEW], so a refused event still shows the view it
/// drew, with the application's reason in a diagnostic ask.
/// </summary>
public sealed unsafe class Library
{
    const string Entry = "osprey_main";
    const string ArgumentExport = "osprey_app_argument";
    const string OpenExport = "osprey_app_open";
    const string EventExport = "osprey_app_event";
    const string BinderPrefix = "osprey_import_";
    const int Unbound = -2;

    readonly delegate* unmanaged<byte*, long> argued;
    readonly delegate* unmanaged<long, long, byte*, byte*, byte, long> opened;
    readonly delegate* unmanaged<long, long, long, byte*, byte*, double, double, byte, byte, long> sent;

    Library(Bridge bridge, nint argument, nint open, nint sent)
    {
        Bridge = bridge;
        argued = (delegate* unmanaged<byte*, long>)(void*)argument;
        opened = (delegate* unmanaged<long, long, byte*, byte*, byte, long>)(void*)open;
        this.sent = (delegate* unmanaged<long, long, long, byte*, byte*, double, double, byte, byte, long>)(void*)sent;
    }

    /// <summary>The shared-memory bridge this DLL carries, opened once before its entry ran.</summary>
    public Bridge Bridge { get; }

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
        Enter(bridge, Export(handle, Entry), path);
        return new Library(bridge, Export(handle, ArgumentExport), Export(handle, OpenExport), Export(handle, EventExport));
    }

    static void Bind(nint handle, string name, nint function)
    {
        if (NativeLibrary.TryGetExport(handle, BinderPrefix + name, out var binder))
        {
            ((delegate* unmanaged<nint, void>)binder)(function);
        }
    }

    static void Enter(Bridge bridge, nint entry, string path)
    {
        var status = ((delegate* unmanaged<int>)entry)();
        // The entry's rows belong to no frame: drop them before the first call publishes one.
        bridge.Discard();
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

    /// <summary>One launch argument, in order, before the first view [NATIVE-SHM-EVENT].</summary>
    public Sealed Argument(string text)
    {
        using var copy = new Utf8Text(text);
        return Sealed.At(Bridge, argued(copy.Pointer));
    }

    /// <summary>Draws the first frame for the host's clock, zone offset, platform, home folder and appearance.</summary>
    public Sealed Open(long now, long offset, string platform, string home, bool dark)
    {
        using var system = new Utf8Text(platform);
        using var folder = new Utf8Text(home);
        return Sealed.At(Bridge, opened(now, offset, system.Pointer, folder.Pointer, dark ? (byte)1 : (byte)0));
    }

    /// <summary>Delivers one event and draws the next frame; a refused event still answers with a whole frame.</summary>
    public Sealed Event(long now, Touch touch)
    {
        using var words = new Utf8Text(touch.Words);
        using var detail = new Utf8Text(touch.Detail);
        return Sealed.At(Bridge, sent(now, touch.Kind, touch.Ordinal, words.Pointer, detail.Pointer, touch.X, touch.Y, touch.Down ? (byte)1 : (byte)0, touch.Shift ? (byte)1 : (byte)0));
    }
}

/// <summary>
/// One call's answer and the frame it wrote, committed and leased whole. It is disposed once the
/// frame has been checked and drawn, releasing all four runs.
/// </summary>
public sealed class Sealed : IDisposable
{
    Sealed(long status, Frame frame)
    {
        Status = status;
        Frame = frame;
    }

    /// <summary>The export's answer: 0 when it accepted the call, 1 or another error when it refused.</summary>
    public long Status { get; }

    /// <summary>The frame every call writes, committed and leased whole.</summary>
    public Frame Frame { get; }

    internal static Sealed At(Bridge bridge, long status)
    {
        bridge.Commit();
        return new Sealed(status, Frame.Lease(bridge));
    }

    /// <summary>Releases the frame's four runs.</summary>
    public void Dispose() => Frame.Dispose();
}

/// <summary>A protocol or boundary failure, shown to the person rather than hidden.</summary>
public sealed class HostException(string message) : Exception(message);
