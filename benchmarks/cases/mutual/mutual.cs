// Mutual recursion — is_even/is_odd counting evens over a loop.
long acc = 0;
for (long i = 1; i < 130000; i++)
{
    if (IsEven(i % 1000))
    {
        acc += 1;
    }
}
System.Console.WriteLine(acc);

static bool IsEven(long n)
{
    if (n == 0)
    {
        return true;
    }
    return IsOdd(n - 1);
}

static bool IsOdd(long n)
{
    if (n == 0)
    {
        return false;
    }
    return IsEven(n - 1);
}
