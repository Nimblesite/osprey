// Word-frequency count over a tiny vocab — exercises hashing + counting.
const long MINSTD = 16807;
const long MODULUS = 2147483647;

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

static long R1(long x)
{
    return (x * MINSTD) % MODULUS;
}

static long HashAt(long s, long i)
{
    return R1(R1(s + i));
}

static long ReadSeed()
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
