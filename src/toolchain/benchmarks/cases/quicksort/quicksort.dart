// Quicksort — naive first-element-pivot quicksort: split the tail into two
// fresh lists around the pivot, sort both, join. Checksum is rank-weighted.

import '../../shared/seed.dart';

const int bigMod = 1000000007;
const int n = 2000;
const int spread = 100000;

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
