// Factorial-style product 1*2*...*N taken mod 1000000007 (matches factorial.osp).
using System;

const long MOD = 1000000007;

long acc = 1;
for (long i = 1; i <= 10000000; i++)
{
    acc = (acc * i) % MOD;
}
Console.WriteLine(acc);
