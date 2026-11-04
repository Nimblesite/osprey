// Mergesort — naive top-down merge sort: deal the run into two fresh halves
// by alternating elements, sort both, merge. Checksum is rank-weighted.
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
    long[] sorted = MergeSort(xs);
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

static long[] MergeSort(long[] xs)
{
    if (xs.Length < 2)
    {
        return xs;
    }
    long[] evens = new long[(xs.Length + 1) / 2];
    long[] odds = new long[xs.Length / 2];
    for (int i = 0; i < xs.Length; i++)
    {
        if (i % 2 == 0)
        {
            evens[i / 2] = xs[i];
        }
        else
        {
            odds[i / 2] = xs[i];
        }
    }
    return Merge(MergeSort(evens), MergeSort(odds));
}

static long[] Merge(long[] left, long[] right)
{
    long[] merged = new long[left.Length + right.Length];
    int i = 0, j = 0, k = 0;
    while (i < left.Length && j < right.Length)
    {
        if (right[j] < left[i])
        {
            merged[k++] = right[j++];
        }
        else
        {
            merged[k++] = left[i++];
        }
    }
    while (i < left.Length)
    {
        merged[k++] = left[i++];
    }
    while (j < right.Length)
    {
        merged[k++] = right[j++];
    }
    return merged;
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
