// Tak function — heavily recursive integer benchmark.
System.Console.WriteLine(Tak(32, 16, 8));

static long Sub(long a, long b)
{
    return a - b;
}

static long Tak(long x, long y, long z)
{
    if (x > y)
    {
        return Tak(
            Tak(Sub(x, 1), y, z),
            Tak(Sub(y, 1), z, x),
            Tak(Sub(z, 1), x, y));
    }
    return z;
}
