// Ackermann–Péter function — deep, non-tail mutual self-recursion.

System.Console.WriteLine(Ack(3, 10));

static long Add(long a, long b) => a + b;
static long Sub(long a, long b) => a - b;

static long Ack(long m, long n)
{
    if (m == 0) return Add(n, 1);
    if (n == 0) return Ack(Sub(m, 1), 1);
    return Ack(Sub(m, 1), Ack(m, Sub(n, 1)));
}
