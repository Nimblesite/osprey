// Factorial-style product 1*2*...*N taken mod 1000000007 (matches factorial.osp).
const int mod = 1000000007;

void main() {
  int acc = 1;
  for (int i = 1; i <= 10000000; i++) {
    acc = (acc * i) % mod;
  }
  print(acc);
}
