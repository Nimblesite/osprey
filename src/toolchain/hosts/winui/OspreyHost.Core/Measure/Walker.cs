using System.Diagnostics;

namespace Osprey.Host.Measure;

/// <summary>One measured child as the slice lists it [UI-EFFECTS-MEASURE].</summary>
internal readonly record struct Entry(string Name, string Path, long Bytes, long Visible, long Items, bool Hidden, bool Folder, bool Link, bool Guarded);

/// <summary>The volume a measured folder is on: its label, capacity and what this user may still write.</summary>
internal readonly record struct Volume(string Name, long Total, long Available);

/// <summary>One measured folder: how its walk went, its totals over every child, its volume and the children it lists.</summary>
internal sealed record Measured(string Path, string Error, bool Missing, bool Truncated, long Unreadable, long Bytes, long Visible, long Items, Volume Volume, IReadOnlyList<Entry> Entries);

/// <summary>Everything below one folder: allocated bytes, the bytes in sight, the items and the folders that could not be read.</summary>
internal readonly record struct Tally(long Bytes, long Visible, long Items, long Unreadable);

/// <summary>A hard-linked file's identity: its volume and its file id, so each file counts once per walk.</summary>
internal readonly record struct FileKey(ulong Volume, UInt128 Id);

/// <summary>The walk's clock and cancellation: a deadline from the start of the request across all its folders.</summary>
internal sealed class Budget(TimeSpan deadline, CancellationToken cancel)
{
    readonly Stopwatch clock = Stopwatch.StartNew();

    /// <summary>Whether the deadline has passed, so the walk descends no further.</summary>
    public bool Expired => clock.Elapsed >= deadline;

    /// <summary>Stops the walk when Osprey cancelled it.</summary>
    public void Check() => cancel.ThrowIfCancellationRequested();
}

/// <summary>
/// Measures the children of one folder to the bottom [UI-EFFECTS-MEASURE] with an explicit stack,
/// so no tree is deep enough to exhaust the thread's stack. Links are reported and never followed,
/// cloud placeholders are never entered, and a hard-linked file counts once.
/// </summary>
internal sealed class Walker(ulong serial, Budget budget, HashSet<FileKey> seen)
{
    readonly byte[] buffer = Listing.Buffer();

    /// <summary>The folder's totals over all its children, and the first <paramref name="limit"/> of them as entries.</summary>
    public Measured Folder(string path, IReadOnlyList<Child> children, int limit, Volume volume)
    {
        var entries = new List<Entry>(Math.Min(limit, children.Count));
        var totals = new Tally();
        var truncated = children.Count > limit;
        foreach (var child in children)
        {
            budget.Check();
            var (entry, unreadable) = Measure(path, child, ref truncated);
            totals = new Tally(totals.Bytes + entry.Bytes, totals.Visible + entry.Visible, totals.Items + 1 + entry.Items, totals.Unreadable + unreadable);
            if (entries.Count < limit)
            {
                entries.Add(entry);
            }
        }
        return new Measured(path, "", false, truncated, totals.Unreadable, totals.Bytes, totals.Visible, totals.Items, volume, entries);
    }

    (Entry Entry, long Unreadable) Measure(string folder, Child child, ref bool truncated)
    {
        var path = Paths.Join(folder, child.Name);
        var own = Own(child);
        var entry = new Entry(child.Name, path, own, child.Hidden ? 0 : own, 0, child.Hidden, child.Directory && !child.Link, child.Link, false);
        if (!entry.Folder || child.Placeholder)
        {
            return (entry, 0);
        }
        if (budget.Expired)
        {
            truncated = true;
            return (entry, 0);
        }
        return Below(path, child.Hidden, ref truncated) is { } below
            ? (entry with { Bytes = own + below.Bytes, Visible = child.Hidden ? 0 : own + below.Visible, Items = below.Items }, below.Unreadable)
            : (entry with { Bytes = 0, Visible = 0, Guarded = true }, 1);
    }

    // The bytes a child adds where it is listed: none for a link, an app alias or a hard link already counted.
    long Own(Child child) => child switch
    {
        { Link: true } or { AppAlias: true } => 0,
        { Directory: true } => child.Allocated,
        _ => seen.Add(new FileKey(serial, child.Id)) ? child.Allocated : 0,
    };

    /// <summary>Everything below the folder at <paramref name="path"/>, or null when that folder cannot be read.</summary>
    Tally? Below(string path, bool hidden, ref bool truncated)
    {
        if (Read(path) is not { } first)
        {
            return null;
        }
        var pending = new Stack<(string Path, bool Hidden)>();
        var tally = Spread(path, hidden, first, pending, new Tally());
        while (pending.TryPop(out var folder))
        {
            budget.Check();
            if (budget.Expired)
            {
                truncated = true;
                return tally;
            }
            tally = Read(folder.Path) is { } children ? Spread(folder.Path, folder.Hidden, children, pending, tally) : tally with { Unreadable = tally.Unreadable + 1 };
        }
        return tally;
    }

    // Counts one listed folder's children and queues the subfolders the walk may enter.
    Tally Spread(string path, bool hidden, List<Child> children, Stack<(string Path, bool Hidden)> pending, Tally tally)
    {
        foreach (var child in children)
        {
            var inSight = !hidden && !child.Hidden;
            var bytes = Own(child);
            tally = new Tally(tally.Bytes + bytes, tally.Visible + (inSight ? bytes : 0), tally.Items + 1, tally.Unreadable);
            if (child.Directory && !child.Link && !child.Placeholder)
            {
                pending.Push((Paths.Join(path, child.Name), !inSight));
            }
        }
        return tally;
    }

    List<Child>? Read(string path)
    {
        using var folder = Listing.Open(path);
        return folder.IsInvalid ? null : Listing.Children(folder, buffer) is ({ } children, 0) ? children : null;
    }
}
