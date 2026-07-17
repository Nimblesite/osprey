// List operations — build arrays, then sum + count below threshold.
import 'dart:io';
import 'dart:math';

const int minstd = 16807;
const int modulus = 2147483647;
const int bigMod = 1000000007;
const int n = 4000;

int r1(int x) => (x * minstd) % modulus;

int hashAt(int s, int i) => r1(r1(s + i));

int readSeed() {
  final line = stdin.readLineSync();
  int m = 0;
  if (line != null) {
    m = int.tryParse(line.trim()) ?? 0;
  }
  if (m == 0) {
    return 1;
  }
  final rng = Random.secure();
  final val = (rng.nextInt(1 << 31) << 31) | rng.nextInt(1 << 31);
  return (val % 2147483646) + 1;
}

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
