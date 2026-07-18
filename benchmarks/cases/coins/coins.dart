// Coin-change ways count — branching recursion over coin kinds.

int coin(int k) {
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

int ways(int amount, int kind) {
  if (amount == 0) return 1;
  if (amount < 0) return 0;
  if (kind == 0) return 0;
  return ways(amount - coin(kind), kind) + ways(amount, kind - 1);
}

void main() {
  print(ways(600, 5));
}
