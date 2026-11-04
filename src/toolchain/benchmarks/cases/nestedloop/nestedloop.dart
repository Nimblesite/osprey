// Triple-nested counting loop accumulating (i*j*k) mod P — nested iteration.
const int p = 1000000007;

int loopK(int i, int j, int k, int acc) {
  while (k != 0) {
    acc = (acc + i * j * k) % p;
    k -= 1;
  }
  return acc;
}

int loopJ(int i, int j, int n, int acc) {
  while (j != 0) {
    acc = loopK(i, j, n, acc);
    j -= 1;
  }
  return acc;
}

int loopI(int i, int n, int acc) {
  while (i != 0) {
    acc = loopJ(i, n, n, acc);
    i -= 1;
  }
  return acc;
}

void main() {
  print(loopI(250, 250, 0));
}
