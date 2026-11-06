// Collatz (3n+1) stopping time summed over 1..N — integer division (n/2) in deep recursion.

int collatz(int n) {
  if (n == 1) return 0;
  return n % 2 == 0 ? 1 + collatz(n ~/ 2) : 1 + collatz(3 * n + 1);
}

void main() {
  int acc = 0;
  for (int i = 1; i < 100001; i++) {
    acc += collatz(i);
  }
  print(acc);
}
