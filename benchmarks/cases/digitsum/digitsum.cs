// Sum of decimal digit-sums over 1..N — integer division (n/10) and modulo in recursion.

long acc = 0;
for (long i = 1; i < 2000001; i++) acc += DigSum(i);
System.Console.WriteLine(acc);

static long DigSum(long n) => n < 10 ? n : (n % 10) + DigSum(n / 10);
