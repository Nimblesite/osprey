// Collatz (3n+1) stopping time summed over 1..N — integer division (n/2) in deep recursion.

long acc = 0;
for (long i = 1; i < 100001; i++) acc += Collatz(i);
System.Console.WriteLine(acc);

static long Collatz(long n)
{
    if (n == 1) return 0;
    return n % 2 == 0 ? 1 + Collatz(n / 2) : 1 + Collatz(3 * n + 1);
}
