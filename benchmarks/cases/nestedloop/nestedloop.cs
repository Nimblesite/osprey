// Triple-nested counting loop accumulating (i*j*k) mod P — nested iteration.
System.Console.WriteLine(LoopI(250, 250, 0));

static long LoopK(long i, long j, long k, long acc)
{
    const long P = 1000000007;
    while (k != 0)
    {
        acc = (acc + i * j * k) % P;
        k -= 1;
    }
    return acc;
}

static long LoopJ(long i, long j, long n, long acc)
{
    while (j != 0)
    {
        acc = LoopK(i, j, n, acc);
        j -= 1;
    }
    return acc;
}

static long LoopI(long i, long n, long acc)
{
    while (i != 0)
    {
        acc = LoopJ(i, n, n, acc);
        i -= 1;
    }
    return acc;
}
