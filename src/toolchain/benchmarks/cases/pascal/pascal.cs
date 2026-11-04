// Binomial coefficient via naive (un-memoised) Pascal recursion C(n,k)=C(n-1,k-1)+C(n-1,k).
System.Console.WriteLine(Binom(27, 13));

static long Binom(long n, long k)
{
    if (k == 0)
    {
        return 1;
    }
    else if (k == n)
    {
        return 1;
    }
    else
    {
        return Binom(n - 1, k - 1) + Binom(n - 1, k);
    }
}
