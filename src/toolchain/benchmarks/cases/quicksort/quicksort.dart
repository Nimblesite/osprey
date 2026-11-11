// Quicksort — naive first-element-pivot quicksort: split the tail into two
// fresh lists around the pivot, sort both, join. Checksum is rank-weighted.

import '../../shared/seed.dart';


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
  print(sortedChecksum(readSeed(), quicksort));
}
