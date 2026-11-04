// Sum of gcd(i, K) for i in 1..N — Euclidean-algorithm recursion (modulo heavy).
int gcd(int a, int b) => b == 0 ? a : gcd(b, a % b);

void main() {
  int acc = 0;
  for (int i = 1; i < 2000000; i++) {
    acc += gcd(i, 1234567);
  }
  print(acc);
}
