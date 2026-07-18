// Naive recursive Fibonacci — exercises raw function-call + recursion overhead.
using System;

Console.WriteLine(Fib(35));

static long Add(long a, long b) => a + b;
static long Sub(long a, long b) => a - b;

static long Fib(long n) => n switch
{
    0 => 0,
    1 => 1,
    _ => Add(Fib(Sub(n, 1)), Fib(Sub(n, 2))),
};
