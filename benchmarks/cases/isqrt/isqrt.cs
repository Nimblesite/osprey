// Sum of integer square roots over 1..N — Newton's method, integer division heavy.
using System;

long acc = 0;
for (long i = 1; i < 1000001; i++)
{
    acc += Isqrt(i);
}
Console.WriteLine(acc);

static long Isqrt(long n)
{
    if (n < 2)
    {
        return n;
    }
    long x = n;
    while (true)
    {
        long y = (x + n / x) / 2;
        if (y < x)
        {
            x = y;
        }
        else
        {
            return x;
        }
    }
}
