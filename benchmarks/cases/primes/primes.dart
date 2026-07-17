// Count primes below a limit by trial division — integer % in a tight loop.
bool hasFactor(int n, int d) {
  if (d * d > n) {
    return false;
  } else if (n % d == 0) {
    return true;
  } else {
    return hasFactor(n, d + 1);
  }
}

bool isPrime(int n) {
  if (n < 2) {
    return false;
  }
  return !hasFactor(n, 2);
}

void main() {
  int acc = 0;
  for (int n = 2; n < 200000; n++) {
    if (isPrime(n)) {
      acc += 1;
    }
  }
  print(acc);
}
