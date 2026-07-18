// Mutual recursion — is_even/is_odd counting evens over a loop.
bool isEven(int n) {
  if (n == 0) {
    return true;
  }
  return isOdd(n - 1);
}

bool isOdd(int n) {
  if (n == 0) {
    return false;
  }
  return isEven(n - 1);
}

void main() {
  int acc = 0;
  for (int i = 1; i < 130000; i++) {
    if (isEven(i % 1000)) {
      acc += 1;
    }
  }
  print(acc);
}
