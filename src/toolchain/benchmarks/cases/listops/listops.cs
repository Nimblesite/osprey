// List operations — build arrays, then sum + count below threshold.
const long MINSTD = 16807;
const long MODULUS = 2147483647;
const long BIG_MOD = 1000000007;
const int N = 4000;

long seed = ReadSeed();

long acc = 0;
for (long t = 0; t < 8; t++)
{
    long s = seed + t * 131;
    long[] xs = new long[N];
    for (long i = 0; i < N; i++)
    {
        xs[i] = HashAt(s, i) % 1000;
    }
    long sum = 0, below = 0;
    for (long i = 0; i < N; i++)
    {
        sum += xs[i];
        if (xs[i] < 500)
        {
            below++;
        }
    }
    acc = (acc + sum + below) % BIG_MOD;
}

System.Console.WriteLine(acc);

static long R1(long x) => (x * MINSTD) % MODULUS;

static long HashAt(long s, long i) => R1(R1(s + i));

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
