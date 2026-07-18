// Josephus problem — survivor index for n people, step k=7, via the modular recurrence.
long acc = 0;
for (long i = 2; i < 10000001; i++)
{
    acc = (acc + 7) % i;
}
System.Console.WriteLine(acc);
