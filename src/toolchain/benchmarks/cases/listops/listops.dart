// List operations — build arrays, then sum + count below threshold.

import '../../shared/seed.dart';

const int bigMod = 1000000007;
const int n = 4000;

void main() {
  final seed = readSeed();

  int acc = 0;
  for (int t = 0; t < 8; t++) {
    final s = seed + t * 131;
    final xs = List<int>.filled(n, 0);
    for (int i = 0; i < n; i++) {
      xs[i] = hashAt(s, i) % 1000;
    }
    int sum = 0, below = 0;
    for (int i = 0; i < n; i++) {
      sum += xs[i];
      if (xs[i] < 500) {
        below++;
      }
    }
    acc = (acc + sum + below) % bigMod;
  }

  print(acc);
}
