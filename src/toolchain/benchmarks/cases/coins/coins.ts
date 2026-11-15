// Coin-change ways count — branching recursion over coin kinds.

function coin(k: number): number {
  switch (k) {
    case 1:
      return 1;
    case 2:
      return 5;
    case 3:
      return 10;
    case 4:
      return 25;
    default:
      return 50;
  }
}

function ways(amount: number, kind: number): number {
  if (amount === 0) return 1;
  if (amount < 0) return 0;
  if (kind === 0) return 0;
  return ways(amount - coin(kind), kind) + ways(amount, kind - 1);
}

function main(): void {
  console.log(ways(600, 5));
}

main();
