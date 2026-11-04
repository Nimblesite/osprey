// Coin-change ways count — branching recursion over coin kinds.

System.Console.WriteLine(Ways(600, 5));

static long Coin(long k) => k switch
{
    1 => 1,
    2 => 5,
    3 => 10,
    4 => 25,
    _ => 50,
};

static long Ways(long amount, long kind)
{
    if (amount == 0) return 1;
    if (amount < 0) return 0;
    if (kind == 0) return 0;
    return Ways(amount - Coin(kind), kind) + Ways(amount, kind - 1);
}
