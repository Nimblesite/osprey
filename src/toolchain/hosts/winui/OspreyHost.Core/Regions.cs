namespace Osprey.Host;

/// <summary>One region as the host holds it [UI-REGIONS]: its component tree at the revision the host issued.</summary>
public sealed record Region(string Key, long Revision, Node Node);

/// <summary>
/// The host's region store [UI-REGIONS] and [NATIVE-SHM-NODES]: the regions of the last told frame,
/// by key. A region whose tree did not change keeps the copy it was drawn from, so its controls are
/// left untouched; a region the frame did not resend is dropped the way main's removals were.
/// </summary>
public sealed class RegionStore
{
    /// <summary>The key of the region every window draws first.</summary>
    public const string Root = Told.Root;

    readonly Dictionary<string, Region> regions = [];

    /// <summary>The region named <paramref name="key"/>, if the application has told one.</summary>
    public Region? Find(string key) => regions.GetValueOrDefault(key);

    /// <summary>Every region the store holds.</summary>
    public IReadOnlyCollection<Region> All => regions.Values;

    /// <summary>
    /// Takes one told frame and returns the keys that changed, patched first: a region whose tree is
    /// the same is untouched, a changed one takes the next revision and a dropped one is removed.
    /// </summary>
    public IReadOnlyList<string> Apply(ToldFrame frame)
    {
        var changed = new List<string>(frame.Regions.Count);
        var live = new HashSet<string>(StringComparer.Ordinal);
        foreach (var (key, node) in frame.Regions)
        {
            live.Add(key);
            if (regions.TryGetValue(key, out var held))
            {
                if (held.Node == node)
                {
                    continue;
                }
                regions[key] = new Region(key, held.Revision + 1, node);
            }
            else
            {
                regions[key] = new Region(key, 0, node);
            }
            changed.Add(key);
        }
        foreach (var key in regions.Keys.Where(key => !live.Contains(key)).ToList())
        {
            regions.Remove(key);
            changed.Add(key);
        }
        return changed;
    }
}
