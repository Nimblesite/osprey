// Quicksort — naive first-element-pivot quicksort: split the tail into two
// fresh runs around the pivot, sort both, join. Checksum is rank-weighted.
const long MINSTD = 16807;
const long MODULUS = 2147483647;
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

static long R1(long x) => (x * MINSTD) % MODULUS;

static long HashAt(long s, long i) => R1(R1(s + i));

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

static long ReadSeed()
{
    string line = System.Console.ReadLine();
    long m = 0;
    if (line != null)
    {
        long parsed;
        if (long.TryParse(line.Trim(), out parsed))
        {
            m = parsed;
        }
    }
    if (m == 0)
    {
        return 1;
    }
    byte[] buf = new byte[8];
    System.Security.Cryptography.RandomNumberGenerator.Fill(buf);
    ulong val = System.BitConverter.ToUInt64(buf, 0);
    return (long)(val % 2147483646UL) + 1;
}
