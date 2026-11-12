using System.Runtime.CompilerServices;

namespace Osprey.Host.Tests;

/// <summary>
/// [NATIVE-SHM-OWNERSHIP] The layout fingerprint and the row width are literals in this host that
/// must track the generated native_abi.h exactly: these tests read the header straight from the
/// repository and fail the moment the two drift apart, so a regenerated layout can never be read
/// wrongly in silence.
/// </summary>
public sealed class NativeAbiTests
{
    const string Header = "src/toolchain/compiler/runtime/native_abi.h";

    static string Defined(string name) =>
        File.ReadAllText(Path.Combine(Framework.Root, Header)).Split('\n').Single(line => line.StartsWith($"#define {name}"));

    [Fact]
    public void TheFingerprintTheHostOpensWithIsTheHeaders()
    {
        var line = Defined("TD_SHARED_FINGERPRINT");
        Assert.Equal(Bridge.Fingerprint, line[(line.IndexOf('"') + 1)..line.LastIndexOf('"')]);
    }

    [Fact]
    public void TheViewRowsTheHostReadsAreAsWideAsTheHeaders()
    {
        var line = File.ReadAllText(Path.Combine(Framework.Root, Header)).Split('\n').Single(text => text.StartsWith("TD_SHARED_STORAGE(ViewRow,"));
        Assert.Equal(long.Parse(line[(line.IndexOf(',') + 1)..line.IndexOf(')')]), Unsafe.SizeOf<NativeView>());
    }
}
