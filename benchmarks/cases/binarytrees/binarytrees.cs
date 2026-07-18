// Allocate + traverse many short-lived binary trees — allocation/memory stress: build 1200 trees of depth 13, sum checks.

long acc = 0;
for (long i = 0; i < 1200; i++)
{
    Tree t = Make(13);
    acc += Check(t);
}
System.Console.WriteLine(acc);

static Tree Make(long d)
{
    Tree t = new Tree();
    if (d != 0)
    {
        t.Left = Make(d - 1);
        t.Right = Make(d - 1);
    }
    return t;
}

static long Check(Tree t)
{
    if (t == null) return 0;
    return 1 + Check(t.Left) + Check(t.Right);
}

class Tree
{
    public Tree Left;
    public Tree Right;
}
