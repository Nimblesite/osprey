// Sum of decimal digit-sums over 1..N — integer division (n/10) and modulo in recursion.

int digsum(int n) => n < 10 ? n : (n % 10) + digsum(n ~/ 10);

void main() {
  int acc = 0;
  for (int i = 1; i < 2000001; i++) {
    acc += digsum(i);
  }
  print(acc);
}
