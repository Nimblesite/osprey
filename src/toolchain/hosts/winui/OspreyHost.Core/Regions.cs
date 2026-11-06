using System.Text.Json;

namespace Osprey.Host;

/// <summary>One region as the host holds it: its node tree at the revision Osprey issued.</summary>
public sealed record Region(string Key, long Revision, JsonElement Node);

/// <summary>
/// The host's region store [UI-REGIONS]: keyed regions replaced by patches and disposed by
/// removals, so a region Osprey did not resend keeps the copy it was drawn from.
/// </summary>
public sealed class RegionStore
{
    /// <summary>The key of the region every window draws first.</summary>
    public const string Root = "root";

    readonly Dictionary<string, Region> regions = [];

    /// <summary>The region named <paramref name="key"/>, if Osprey has sent one.</summary>
    public Region? Find(string key) => regions.GetValueOrDefault(key);

    /// <summary>Every region the store holds.</summary>
    public IReadOnlyCollection<Region> All => regions.Values;

    /// <summary>Applies one envelope's patches and removals and returns the keys that changed, patched first.</summary>
    public IReadOnlyList<string> Apply(Envelope envelope)
    {
        foreach (var patch in envelope.Patches)
        {
            regions[patch.Key] = new Region(patch.Key, patch.Revision, patch.Node);
        }
        foreach (var key in envelope.Removed)
        {
            regions.Remove(key);
        }
        return [.. envelope.Patches.Select(patch => patch.Key), .. envelope.Removed];
    }
}
