// Text statistics over a tiny vocab — exercises length/char-search per word.
const long MINSTD = 16807;
const long MODULUS = 2147483647;

string[] vocab = { "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog" };
long seed = ReadSeed();

long acc = 0;
for (long i = 0; i < 200000; i++)
{
    string w = vocab[HashAt(seed, i) % 8];
    acc += (long)w.Length
         + (Contains(w, 'o') ? 7 : 0)
         + (w[0] == 't' ? 3 : 0);
}

System.Console.WriteLine(acc);

static long R1(long x)
{
    return (x * MINSTD) % MODULUS;
}

static long HashAt(long s, long i)
{
    return R1(R1(s + i));
}

static bool Contains(string w, char c)
{
    for (int j = 0; j < w.Length; j++)
    {
        if (w[j] == c)
        {
            return true;
        }
    }
    return false;
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
