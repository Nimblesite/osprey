// Sum of modular exponentiation: sum of i^20 mod P for i in 1..N, naive repeated multiply.
const long P = 1000000007;

long acc = 0;
for (long i = 1; i < 1000000; i++)
{
    acc = (acc + PowMod(i, 20, 1)) % P;
}
System.Console.WriteLine(acc);

static long PowMod(long b, long e, long acc)
{
    if (e == 0)
    {
        return acc;
    }
    return PowMod(b, e - 1, (acc * b) % P);
}
