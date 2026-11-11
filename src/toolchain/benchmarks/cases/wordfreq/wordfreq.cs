// Word-frequency count over a tiny vocab — exercises hashing + counting.
using static Seed;

long seed = ReadSeed();

long[] counts = new long[8];
for (long i = 0; i < 200000; i++)
{
    counts[HashAt(seed, i) % 8] += 1;
}

long result = 0;
for (long k = 0; k < 8; k++)
{
    result += (k + 1) * counts[k];
}

System.Console.WriteLine(result);
