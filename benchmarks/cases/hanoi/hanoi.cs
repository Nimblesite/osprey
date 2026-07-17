// Towers of Hanoi move count via double recursion with an accumulator.
using System;

Console.WriteLine(Hanoi(25, 0));

static long Hanoi(long n, long acc)
{
    if (n == 0)
    {
        return acc;
    }
    return Hanoi(n - 1, Hanoi(n - 1, acc) + 1);
}
