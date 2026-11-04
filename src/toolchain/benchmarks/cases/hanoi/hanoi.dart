// Towers of Hanoi move count via double recursion with an accumulator.
int hanoi(int n, int acc) {
  if (n == 0) {
    return acc;
  }
  return hanoi(n - 1, hanoi(n - 1, acc) + 1);
}

void main() {
  print(hanoi(25, 0));
}
