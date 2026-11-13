using System.Runtime.CompilerServices;

namespace Osprey.Host.Tests;

/// <summary>
/// [NATIVE-SHM-OWNERSHIP] The layout fingerprint and every row width are literals in this host that
/// must track the generated native_abi.h exactly: these tests read the header straight from the
/// repository and fail the moment the two drift apart, so a regenerated layout can never be read
/// wrongly in silence.
/// </summary>
public sealed class NativeAbiTests
{
    const string Header = "src/toolchain/compiler/runtime/native_abi.h";

    static string Defined(string name) =>
        File.ReadAllText(Path.Combine(Framework.Root, Header)).Split('\n').Single(line => line.StartsWith($"#define {name}"));

    static long Width(string storage)
    {
        var line = File.ReadAllText(Path.Combine(Framework.Root, Header)).Split('\n')
            .Single(text => text.StartsWith($"TD_SHARED_STORAGE({storage},"));
        return long.Parse(line[(line.IndexOf(',') + 1)..line.IndexOf(')')]);
    }

    [Fact]
    public void TheFingerprintTheHostOpensWithIsTheHeaders()
    {
        var line = Defined("TD_SHARED_FINGERPRINT_64");
        Assert.Equal(Bridge.Fingerprint, line[(line.IndexOf('"') + 1)..line.LastIndexOf('"')]);
    }

    [Fact]
    public void EveryRowTheHostReadsIsAsWideAsTheHeaders()
    {
        Assert.Equal(Width("SceneShape"), Unsafe.SizeOf<NativeShape>());
        Assert.Equal(Width("ViewRow"), Unsafe.SizeOf<NativeView>());
        Assert.Equal(Width("AskRow"), Unsafe.SizeOf<NativeAsk>());
    }

    [Fact]
    public void EveryRowFieldTheHostReadsIsWhereTheHeaderPutsIt()
    {
        Assert.Equal(0, (int)System.Runtime.InteropServices.Marshal.OffsetOf<NativeShape>(nameof(NativeShape.Code)));
        Assert.Equal(72, (int)System.Runtime.InteropServices.Marshal.OffsetOf<NativeShape>(nameof(NativeShape.TextOffset)));
        Assert.Equal(80, (int)System.Runtime.InteropServices.Marshal.OffsetOf<NativeShape>(nameof(NativeShape.TextLength)));
        Assert.Equal(24, (int)System.Runtime.InteropServices.Marshal.OffsetOf<NativeView>(nameof(NativeView.TextOffset)));
        Assert.Equal(120, (int)System.Runtime.InteropServices.Marshal.OffsetOf<NativeView>(nameof(NativeView.Flags)));
        Assert.Equal(144, (int)System.Runtime.InteropServices.Marshal.OffsetOf<NativeView>(nameof(NativeView.HintLength)));
        Assert.Equal(8, (int)System.Runtime.InteropServices.Marshal.OffsetOf<NativeAsk>(nameof(NativeAsk.Number)));
        Assert.Equal(16, (int)System.Runtime.InteropServices.Marshal.OffsetOf<NativeAsk>(nameof(NativeAsk.TextOffset)));
        Assert.Equal(24, (int)System.Runtime.InteropServices.Marshal.OffsetOf<NativeAsk>(nameof(NativeAsk.TextLength)));
    }
}
