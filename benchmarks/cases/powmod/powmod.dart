// Sum of modular exponentiation: sum of i^20 mod P for i in 1..N, naive repeated multiply.
const int P = 1000000007;

int powmod(int base, int e, int acc) {
  if (e == 0) {
    return acc;
  }
  return powmod(base, e - 1, (acc * base) % P);
}

void main() {
  int acc = 0;
  for (int i = 1; i < 1000000; i++) {
    acc = (acc + powmod(i, 20, 1)) % P;
  }
  print(acc);
}
