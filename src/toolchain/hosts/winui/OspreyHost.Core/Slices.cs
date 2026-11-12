using System.Runtime.InteropServices;
using System.Text;

namespace Osprey.Host;

/// <summary>
/// Host-held slices [UI-STORE]: text the application wrote, or an adapter produced, kept by key
/// and answered through the <c>native_slice</c> import while Osprey runs. A buffer handed to
/// Osprey is never freed, because the runtime may keep the pointer; a replaced buffer is retired.
/// </summary>
public static unsafe class Slices
{
    static readonly Dictionary<string, nint> Held = [];
    static readonly List<nint> Retired = [];
    static readonly nint Empty = Allocate("");

    /// <summary>Holds <paramref name="text"/> under <paramref name="key"/> for the next fetch.</summary>
    public static void Put(string key, string text)
    {
        if (text.Contains('\0'))
        {
            throw new HostException($"Slice {key} contains a NUL character, which no C string can carry");
        }
        if (Held.TryGetValue(key, out var previous))
        {
            Retired.Add(previous);
        }
        Held[key] = Allocate(text);
    }

    /// <summary>The text held under <paramref name="key"/>, or null when nothing was written.</summary>
    public static string? Get(string key) =>
        Held.TryGetValue(key, out var text) ? Marshal.PtrToStringUTF8(text) : null;

    /// <summary>The <c>native_slice</c> import: the text under the key, or the empty string.</summary>
    [UnmanagedCallersOnly]
    public static byte* Answer(byte* key)
    {
        var name = key is null ? null : Marshal.PtrToStringUTF8((nint)key);
        return (byte*)(name is not null && Held.TryGetValue(name, out var text) ? text : Empty);
    }

    static nint Allocate(string text)
    {
        var bytes = Encoding.UTF8.GetBytes(text);
        var buffer = (byte*)NativeMemory.Alloc((nuint)bytes.Length + 1);
        bytes.CopyTo(new Span<byte>(buffer, bytes.Length));
        buffer[bytes.Length] = 0;
        return (nint)buffer;
    }
}

/// <summary>A NUL-terminated UTF-8 copy of a string, freed when the call it was made for returns.</summary>
public readonly unsafe struct Utf8Text : IDisposable
{
    /// <summary>The copy, valid until <see cref="Dispose"/>.</summary>
    public byte* Pointer { get; }

    /// <summary>Copies <paramref name="text"/>; text holding a NUL cannot cross the boundary.</summary>
    public Utf8Text(string text)
    {
        if (text.Contains('\0'))
        {
            throw new HostException("Host text contained a NUL character, which no C string can carry");
        }
        var bytes = Encoding.UTF8.GetBytes(text);
        Pointer = (byte*)NativeMemory.Alloc((nuint)bytes.Length + 1);
        bytes.CopyTo(new Span<byte>(Pointer, bytes.Length));
        Pointer[bytes.Length] = 0;
    }

    /// <summary>Frees the copy.</summary>
    public void Dispose() => NativeMemory.Free(Pointer);
}
