// Count primes below a limit by trial division — integer % in a tight loop.
long acc = 0;
for (long n = 2; n < 200000; n++)
{
    if (IsPrime(n))
    {
        acc += 1;
    }
}
System.Console.WriteLine(acc);

static bool HasFactor(long n, long d)
{
    if (d * d > n)
    {
        return false;
    }
    else if (n % d == 0)
    {
        return true;
    }
    else
    {
        return HasFactor(n, d + 1);
    }
}

static bool IsPrime(long n)
{
    if (n < 2)
    {
        return false;
    }
    return !HasFactor(n, 2);
}
