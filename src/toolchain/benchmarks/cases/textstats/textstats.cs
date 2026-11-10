// Text statistics over a tiny vocab — exercises length/char-search per word.
using static Seed;

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
