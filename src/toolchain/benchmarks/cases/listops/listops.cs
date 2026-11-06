// List operations — build arrays, then sum + count below threshold.
using static Seed;
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
