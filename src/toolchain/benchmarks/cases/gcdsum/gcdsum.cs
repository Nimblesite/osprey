// Sum of gcd(i, K) for i in 1..N — Euclidean-algorithm recursion (modulo heavy).
using System;

long acc = 0;
for (long i = 1; i < 2000000; i++)
{
    acc += Gcd(i, 1234567);
}
Console.WriteLine(acc);

static long Gcd(long a, long b) => b == 0 ? a : Gcd(b, a % b);
