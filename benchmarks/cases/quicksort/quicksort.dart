// Quicksort — naive first-element-pivot quicksort: split the tail into two
// fresh lists around the pivot, sort both, join. Checksum is rank-weighted.
import 'dart:io';
import 'dart:math';

const int minstd = 16807;
const int modulus = 2147483647;
const int bigMod = 1000000007;
const int n = 2000;
const int spread = 100000;

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

List<int> quicksort(List<int> xs) {
  if (xs.length < 2) {
    return xs;
  }
  final pivot = xs.first;
  final below = <int>[];
  final atLeast = <int>[];
  for (int i = 1; i < xs.length; i++) {
    if (xs[i] < pivot) {
      below.add(xs[i]);
    } else {
      atLeast.add(xs[i]);
    }
  }
  return [...quicksort(below), pivot, ...quicksort(atLeast)];
}

void main() {
  final seed = readSeed();

  int acc = 0;
  for (int t = 0; t < 8; t++) {
    final s = seed + t * 131;
    final xs = List<int>.generate(n, (i) => hashAt(s, i) % spread);
    final sorted = quicksort(xs);
    int weight = 0;
    for (int i = 0; i < n; i++) {
      weight = (weight + sorted[i] * (i + 1)) % bigMod;
    }
    acc = (acc + weight) % bigMod;
  }

  print(acc);
}
