// Expression tree — build a heap-allocated tree, then evaluate it.
using System;

const long MINSTD = 16807;
const long MODULUS = 2147483647;
const long BIG_MOD = 1000000007;

long seed = ReadSeed();

long acc = 0;
for (long t = 0; t < 10; t++)
{
    acc = (acc + Eval(Build(seed + t * 7, 1, 14))) % BIG_MOD;
}

Console.WriteLine(acc);

static long R1(long x) => (x * MINSTD) % MODULUS;

static long HashAt(long s, long i) => R1(R1(s + i));

static long ReadSeed()
{
    long m = 0;
    string line = Console.ReadLine();
    if (line != null && long.TryParse(line.Trim(), out long parsed))
    {
        m = parsed;
    }
    if (m == 0)
    {
        return 1;
    }
    ulong val = 0;
    try
    {
        byte[] buf = new byte[8];
        using var f = System.IO.File.OpenRead("/dev/urandom");
        if (f.Read(buf, 0, 8) == 8)
        {
            val = BitConverter.ToUInt64(buf, 0);
        }
    }
    catch (Exception)
    {
        val = 0;
    }
    return (long)(val % 2147483646) + 1;
}

static Expr Build(long s, long idx, long depth)
{
    if (depth == 0)
    {
        return new Expr { Tag = Expr.TagLit, V = HashAt(s, idx) % 1000 };
    }
    long op = HashAt(s, idx) % 2;
    var e = new Expr
    {
        V = 0,
        L = Build(s, idx * 2 + 1, depth - 1),
        R = Build(s, idx * 2 + 2, depth - 1),
    };
    e.Tag = op == 0 ? Expr.TagAdd : Expr.TagMul;
    return e;
}

static long Eval(Expr e) => e.Tag switch
{
    Expr.TagLit => e.V,
    Expr.TagAdd => (Eval(e.L) + Eval(e.R)) % BIG_MOD,
    _ => (Eval(e.L) * Eval(e.R)) % BIG_MOD,
};

class Expr
{
    public const int TagLit = 0;
    public const int TagAdd = 1;
    public const int TagMul = 2;

    public int Tag;
    public long V;
    public Expr L, R;
}
