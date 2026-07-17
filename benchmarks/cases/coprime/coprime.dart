// Count coprime pairs (i,j), 1<=i,j<=N — nested iteration + Euclidean gcd.

int gcd(int a, int b) => b == 0 ? a : gcd(b, a % b);

void main() {
  const n = 2000;
  int acc = 0;
  for (int i = n; i > 0; i--) {
    for (int j = n; j > 0; j--) {
      if (gcd(i, j) == 1) {
        acc++;
      }
    }
  }
  print(acc);
}
