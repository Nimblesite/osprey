// Sum of integer square roots over 1..N — Newton's method, integer division heavy.
int isqrt(int n) {
  if (n < 2) {
    return n;
  }
  int x = n;
  while (true) {
    final y = (x + n ~/ x) ~/ 2;
    if (y < x) {
      x = y;
    } else {
      return x;
    }
  }
}

void main() {
  int acc = 0;
  for (int i = 1; i < 1000001; i++) {
    acc += isqrt(i);
  }
  print(acc);
}
