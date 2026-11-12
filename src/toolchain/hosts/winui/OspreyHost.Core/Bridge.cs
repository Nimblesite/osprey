using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Security.Cryptography;

namespace Osprey.Host;

/// <summary>One shape a scene streamed during a call [NATIVE-SHM-SCENE], laid out as the C <c>OspNativeShape</c> of native_bridge.h.</summary>
[StructLayout(LayoutKind.Sequential)]
public readonly record struct NativeShape(long Code, long Index, double X, double Y, double Width, double Height, double Angle, long Colour, long Depth);

/// <summary>One native control [NATIVE-SHM-VIEW], laid out as the C <c>OspNativeView</c> of native_bridge.h: text and hint are slices of the frame's text run. The tests read native_abi.h from the repository and fail if this layout drifts from it.</summary>
[StructLayout(LayoutKind.Sequential)]
public readonly struct NativeView
{
    public readonly long Kind, Parent, Action, TextOffset, TextLength, Colour, Background;
    public readonly double Size, Padding, Spacing, Radius;
    public readonly long Weight;
    public readonly double Width, Height, Grow; // Fixed extents (0 = natural) and flex share.
    public readonly long Flags, Border, HintOffset, HintLength;
}

/// <summary>The opaque 64 bytes of the C <c>td_OwnedLease</c>: an owner's hold on one run.</summary>
[StructLayout(LayoutKind.Sequential)]
internal unsafe struct Lease
{
    internal fixed ulong Words[8];
}

/// <summary>The 16 random bytes of the C <c>td_ApplicationInstance</c>: everything this host allocates on the bridge names them.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct Instance
{
    internal ulong First, Second;
}

/// <summary>
/// The shared-memory bridge an application DLL exports as one table of function pointers
/// [NATIVE-SHM-OWNERSHIP/PUBLISH]: <c>osprey_native_bridge</c>, twelve entries in the order
/// native_bridge.h declares them. Opened once per application before its entry runs; after every
/// export the host seals the frame the call wrote and leases the runs it needs, reading them in
/// place: no copy, no wire format [NATIVE-SHM-VIEW].
/// </summary>
public sealed unsafe class Bridge
{
    /// <summary>The child a published frame keeps its streamed scene shapes in.</summary>
    public const int ShapesRun = 0;

    /// <summary>The child a published frame keeps its view rows in.</summary>
    public const int ViewsRun = 1;

    /// <summary>The child a published frame keeps its view rows' text in.</summary>
    public const int TextRun = 2;

    const int Entries = 12;
    const string Table = "osprey_native_bridge";
    // TD_SHARED_FINGERPRINT of native_abi.h: the row layouts both sides must agree on. The tests
    // read the header from the repository and fail the moment this literal differs from it.
    internal const string Fingerprint = "33856a05f72962856b37399492572d6d96aa8ea0a252d011802a10d24c0d45be";
    const long Bytes = 1 << 24, Chunks = 256, Owners = 256;
    // One name per generated BridgeError discriminant, in tag order: a status is one plus its tag.
    static readonly string[] Faults =
    [
        "an invalid limit", "a limit exceeded", "it would block", "the wrong instance",
        "a stale allocation", "an invalid owner", "an invalid range", "an invalid layout",
        "an exhausted counter", "the bridge is closing", "the bridge is closed",
        "a failed allocation", "failed synchronization",
    ];

    readonly void* bridge;
    readonly delegate* unmanaged<void*, long*, int> commit;
    readonly delegate* unmanaged<void> discard;
    readonly delegate* unmanaged<void*, Lease*, long*, int> acquire;
    readonly delegate* unmanaged<void*, Lease*, long, Lease*, int> child;
    readonly delegate* unmanaged<void*, Lease*, long*, int> length;
    readonly delegate* unmanaged<void*, Lease*, long, long, long, void**, int> view;
    readonly delegate* unmanaged<void*, Lease*, int> release;
    readonly delegate* unmanaged<void*, nuint*, nuint*, nuint*, int> usage;

    Bridge(void* opened, void** table)
    {
        bridge = opened;
        commit = (delegate* unmanaged<void*, long*, int>)table[1];
        discard = (delegate* unmanaged<void>)table[2];
        acquire = (delegate* unmanaged<void*, Lease*, long*, int>)table[3];
        child = (delegate* unmanaged<void*, Lease*, long, Lease*, int>)table[4];
        length = (delegate* unmanaged<void*, Lease*, long*, int>)table[5];
        view = (delegate* unmanaged<void*, Lease*, long, long, long, void**, int>)table[6];
        release = (delegate* unmanaged<void*, Lease*, int>)table[8];
        usage = (delegate* unmanaged<void*, nuint*, nuint*, nuint*, int>)table[11];
    }

    /// <summary>Opens the bridge the DLL at <paramref name="library"/> exports, once per application: the fingerprint's storage, a random instance, and its leases taken from here on.</summary>
    public static Bridge Open(nint library)
    {
        Layout();
        if (!NativeLibrary.TryGetExport(library, Table, out var address))
        {
            throw new HostException($"The application exports no {Table} table of shared-memory bridge entries");
        }
        var entries = stackalloc void*[Entries];
        for (var index = 0; index < Entries; index++)
        {
            entries[index] = (void*)Marshal.ReadIntPtr(address, index * nint.Size);
        }
        var open = (delegate* unmanaged<Instance*, byte*, long, long, long, void**, int>)entries[0];
        return new Bridge(Opened(open), entries);
    }

    static void* Opened(delegate* unmanaged<Instance*, byte*, long, long, long, void**, int> open)
    {
        var seed = RandomNumberGenerator.GetBytes(16);
        var instance = new Instance { First = BitConverter.ToUInt64(seed), Second = BitConverter.ToUInt64(seed, 8) };
        using var fingerprint = new Utf8Text(Fingerprint);
        void* opened = null;
        Checked(open(&instance, fingerprint.Pointer, Bytes, Chunks, Owners, &opened), "opening the bridge");
        return opened;
    }

    // The rows are read straight out of the application's memory: a layout that drifted would be
    // read wrongly, so it is refused before any pointer is taken.
    static void Layout()
    {
        if (sizeof(NativeShape) != 72 || sizeof(NativeView) != 152 || sizeof(Lease) != 64 || sizeof(Instance) != 16)
        {
            throw new HostException("This host's bridge rows no longer match native_bridge.h");
        }
    }

    /// <summary>Seals the frame the last export wrote and publishes it; the pending runs are consumed whether or not this succeeds.</summary>
    public long Commit()
    {
        long revision = 0;
        Checked(commit(bridge, &revision), "sealing the frame");
        return revision;
    }

    /// <summary>Drops the partial frame of a call that failed: it belongs to nothing.</summary>
    public void Discard() => discard();

    /// <summary>
    /// The run at <paramref name="index"/> of the published frame, leased to be read in place until
    /// <see cref="Run{T}.Dispose"/> releases it. The frame's own lease is taken and dropped here,
    /// as the reference host does; the run's chunk stays pinned by the lease it answers.
    /// </summary>
    public Run<T> Take<T>(int index) where T : unmanaged
    {
        Lease frame = default, lease = default;
        long revision = 0, bytes = 0;
        void* data = null;
        Checked(acquire(bridge, &frame, &revision), "acquiring the published frame");
        var chosen = child(bridge, &frame, index, &lease);
        var freed = release(bridge, &frame);
        Checked(chosen, $"taking the frame's child {index}");
        Checked(freed, "releasing the frame lease");
        Checked(length(bridge, &lease, &bytes), "measuring the run");
        Checked(view(bridge, &lease, index + 1, 0, bytes, &data), $"viewing the frame's child {index}");
        return new Run<T>(this, lease, data, bytes);
    }

    /// <summary>The storage the bridge holds: its bytes, chunks and owners now.</summary>
    public (long Bytes, long Chunks, long Owners) Usage()
    {
        nuint bytes = 0, chunks = 0, owners = 0;
        Checked(usage(bridge, &bytes, &chunks, &owners), "reading the bridge's usage");
        return ((long)bytes, (long)chunks, (long)owners);
    }

    internal void Released(in Lease lease)
    {
        var dying = lease;
        Checked(release(bridge, &dying), "releasing a run's lease");
    }

    static void Checked(int status, string what)
    {
        if (status != 0)
        {
            throw new HostException($"The shared-memory bridge refused {what}: {Fault(status)}");
        }
    }

    static string Fault(long status) => status > 0 && status <= Faults.Length ? Faults[status - 1] : $"error {status}";
}

/// <summary>
/// One leased run of a published frame [NATIVE-SHM-VIEW]: its rows and bytes, read in place from
/// the memory the application wrote. The spans are valid only until <see cref="Dispose"/> releases
/// the lease, so a new run is leased before an old one is released.
/// </summary>
public sealed unsafe class Run<T> : IDisposable where T : unmanaged
{
    readonly Bridge bridge;
    readonly Lease lease;
    readonly void* data;
    bool released;

    internal Run(Bridge bridge, Lease lease, void* data, long bytes)
    {
        if (bytes % Unsafe.SizeOf<T>() != 0)
        {
            throw new HostException($"A run holds {bytes} bytes, which is not whole rows of {Unsafe.SizeOf<T>()}");
        }
        this.bridge = bridge;
        this.lease = lease;
        this.data = data;
        Length = bytes;
    }

    /// <summary>The run's length in bytes.</summary>
    public long Length { get; }

    /// <summary>The run's rows, read in place; valid until <see cref="Dispose"/>.</summary>
    public ReadOnlySpan<T> Rows => new(data, checked((int)(Length / Unsafe.SizeOf<T>())));

    /// <summary>The run's raw bytes; valid until <see cref="Dispose"/>.</summary>
    public ReadOnlySpan<byte> Bytes => new(data, checked((int)Length));

    /// <summary>Releases the lease; the spans must not be read after this.</summary>
    public void Dispose()
    {
        if (!released)
        {
            released = true;
            bridge.Released(lease);
        }
    }
}
