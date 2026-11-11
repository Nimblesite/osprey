// Quicksort — naive first-element-pivot quicksort: split the tail into two
// fresh runs around the pivot, sort both, join. Checksum is rank-weighted.
using static Seed;
System.Console.WriteLine(SortedChecksum(ReadSeed(), Quicksort));

static long[] Quicksort(long[] xs)
{
    if (xs.Length < 2)
    {
        return xs;
    }
    long pivot = xs[0];
    var below = new System.Collections.Generic.List<long>();
    var atLeast = new System.Collections.Generic.List<long>();
    for (int i = 1; i < xs.Length; i++)
    {
        if (xs[i] < pivot)
        {
            below.Add(xs[i]);
        }
        else
        {
            atLeast.Add(xs[i]);
        }
    }
    long[] left = Quicksort(below.ToArray());
    long[] right = Quicksort(atLeast.ToArray());
    long[] sorted = new long[xs.Length];
    left.CopyTo(sorted, 0);
    sorted[left.Length] = pivot;
    right.CopyTo(sorted, left.Length + 1);
    return sorted;
}
