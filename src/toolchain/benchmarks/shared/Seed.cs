// The seed and hash the list and text cases share. run.sh compiles this file
// beside each case's Program.cs, so every case stays one NativeAOT binary.
static class Seed
{
    const long MINSTD = 16807;
    const long MODULUS = 2147483647;

    public static long R1(long x) => (x * MINSTD) % MODULUS;

    public static long HashAt(long s, long i) => R1(R1(s + i));

    // Reads the seed's leading integer as C's strtoll does: 0 selects the
    // fixed seed 1, anything else a random one.
    public static long ReadSeed()
    {
        string line = System.Console.ReadLine();
        long m = 0;
        if (line != null)
        {
            int end = 0;
            while (end < line.Length && (line[end] == ' ' || line[end] == '\t')) end++;
            int start = end;
            if (end < line.Length && (line[end] == '+' || line[end] == '-')) end++;
            while (end < line.Length && line[end] >= '0' && line[end] <= '9') end++;
            if (end > start)
            {
                long.TryParse(line.Substring(start, end - start), out m);
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

    // The sort cases' shared run: eight seeded runs of 2000 values below
    // 100000, each sorted by `sort` and folded into one rank-weighted checksum.
    public static long SortedChecksum(long seed, System.Func<long[], long[]> sort)
    {
        const long bigMod = 1000000007;
        long acc = 0;
        for (long t = 0; t < 8; t++)
        {
            long[] xs = new long[2000];
            for (long i = 0; i < xs.Length; i++)
            {
                xs[i] = HashAt(seed + t * 131, i) % 100000;
            }
            long[] sorted = sort(xs);
            long weight = 0;
            for (long i = 0; i < sorted.Length; i++)
            {
                weight = (weight + sorted[i] * (i + 1)) % bigMod;
            }
            acc = (acc + weight) % bigMod;
        }
        return acc;
    }
}
