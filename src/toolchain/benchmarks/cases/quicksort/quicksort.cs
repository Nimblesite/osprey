// Quicksort — naive first-element-pivot quicksort: split the tail into two
// fresh runs around the pivot, sort both, join. Checksum is rank-weighted.
using static Seed;
const long BIG_MOD = 1000000007;
const int N = 2000;
const long SPREAD = 100000;

long seed = ReadSeed();

long acc = 0;
for (long t = 0; t < 8; t++)
{
    long s = seed + t * 131;
    long[] xs = new long[N];
    for (long i = 0; i < N; i++)
    {
        xs[i] = HashAt(s, i) % SPREAD;
    }
    long[] sorted = Quicksort(xs);
    long weight = 0;
    for (long i = 0; i < N; i++)
    {
        weight = (weight + sorted[i] * (i + 1)) % BIG_MOD;
    }
    acc = (acc + weight) % BIG_MOD;
}

System.Console.WriteLine(acc);

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
