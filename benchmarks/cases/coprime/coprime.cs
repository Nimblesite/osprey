// Count coprime pairs (i,j), 1<=i,j<=N — nested iteration + Euclidean gcd.

long n = 2000;
long acc = 0;
for (long i = n; i > 0; i--)
{
    for (long j = n; j > 0; j--)
    {
        if (Gcd(i, j) == 1) acc++;
    }
}
System.Console.WriteLine(acc);

static long Gcd(long a, long b) => b == 0 ? a : Gcd(b, a % b);
