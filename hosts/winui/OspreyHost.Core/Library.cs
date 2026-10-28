using System.Runtime.InteropServices;

namespace Osprey.Host;

/// <summary>
/// The Osprey application DLL [WINUI-HOST-DLL]: its entry, its <c>start</c> and <c>dispatch</c>
/// exports, and the import slots it calls back through [WINUI-HOST-IMPORTS]. Every call is made
/// on the UI thread [WINUI-HOST-THREAD], and a returned string is copied before the next call,
/// because it belongs to Osprey.
/// </summary>
public sealed unsafe class Library
{
    const string Entry = "osprey_main";
    const string StartExport = "osprey_app_start";
    const string DispatchExport = "osprey_app_dispatch";
    const string BinderPrefix = "osprey_import_";
    const int Unbound = -2;

    readonly delegate* unmanaged<byte*, byte*> start;
    readonly delegate* unmanaged<byte*, byte*, byte*> dispatch;

    Library(nint start, nint dispatch)
    {
        this.start = (delegate* unmanaged<byte*, byte*>)start;
        this.dispatch = (delegate* unmanaged<byte*, byte*, byte*>)dispatch;
    }

    /// <summary>The imports this host answers, by the name an application declares them with.</summary>
    public static IReadOnlyDictionary<string, nint> Imports { get; } = new Dictionary<string, nint>
    {
        ["native_slice"] = (nint)(delegate* unmanaged<byte*, byte*>)&Slices.Answer,
        ["native_shape"] = (nint)(delegate* unmanaged<long, long, double, double, double, double, double, long, long, long>)&Shapes.Answer,
    };

    /// <summary>Loads the application, fills every slot it declares and runs its entry once.</summary>
    public static Library Open(string path)
    {
        var handle = NativeLibrary.Load(Path.GetFullPath(path));
        foreach (var (name, function) in Imports)
        {
            Bind(handle, name, function);
        }
        Enter(Export(handle, Entry), path);
        return new Library(Export(handle, StartExport), Export(handle, DispatchExport));
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

    /// <summary>The start envelope for the launch <paramref name="context"/>.</summary>
    public string Start(string context)
    {
        Shapes.Clear();
        using var source = new Utf8Text(context);
        return Copied(start(source.Pointer));
    }

    /// <summary>The next envelope for the opaque <paramref name="state"/> and one <paramref name="eventText"/>.</summary>
    public string Dispatch(string state, string eventText)
    {
        Shapes.Clear();
        using var saved = new Utf8Text(state);
        using var message = new Utf8Text(eventText);
        return Copied(dispatch(saved.Pointer, message.Pointer));
    }

    static string Copied(byte* value) =>
        value is null
            ? throw new HostException("The Osprey application returned no envelope")
            : Marshal.PtrToStringUTF8((nint)value) ?? "";
}

/// <summary>A protocol or boundary failure, shown to the person rather than hidden.</summary>
public sealed class HostException(string message) : Exception(message);
