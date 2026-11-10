// Mergesort — naive top-down merge sort: deal the run into two fresh halves
// by alternating elements, sort both, merge. Checksum is rank-weighted.
using static Seed;
System.Console.WriteLine(SortedChecksum(ReadSeed(), MergeSort));

static long[] MergeSort(long[] xs)
{
    if (xs.Length < 2)
    {
        return xs;
    }
    long[] evens = new long[(xs.Length + 1) / 2];
    long[] odds = new long[xs.Length / 2];
    for (int i = 0; i < xs.Length; i++)
    {
        if (i % 2 == 0)
        {
            evens[i / 2] = xs[i];
        }
        else
        {
            odds[i / 2] = xs[i];
        }
    }
    return Merge(MergeSort(evens), MergeSort(odds));
}

static long[] Merge(long[] left, long[] right)
{
    long[] merged = new long[left.Length + right.Length];
    int i = 0, j = 0, k = 0;
    while (i < left.Length && j < right.Length)
    {
        if (right[j] < left[i])
        {
            merged[k++] = right[j++];
        }
        else
        {
            merged[k++] = left[i++];
        }
    }
    while (i < left.Length)
    {
        merged[k++] = left[i++];
    }
    while (j < right.Length)
    {
        merged[k++] = right[j++];
    }
    return merged;
}
