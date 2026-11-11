// Mergesort — naive top-down merge sort: deal the list into two fresh halves
// by alternating elements, sort both, merge. Checksum is rank-weighted.

import '../../shared/seed.dart';


List<int> merge(List<int> left, List<int> right) {
  final merged = <int>[];
  int i = 0, j = 0;
  while (i < left.length && j < right.length) {
    if (right[j] < left[i]) {
      merged.add(right[j++]);
    } else {
      merged.add(left[i++]);
    }
  }
  while (i < left.length) {
    merged.add(left[i++]);
  }
  while (j < right.length) {
    merged.add(right[j++]);
  }
  return merged;
}

List<int> mergeSort(List<int> xs) {
  if (xs.length < 2) {
    return xs;
  }
  final evens = <int>[];
  final odds = <int>[];
  for (int i = 0; i < xs.length; i++) {
    if (i % 2 == 0) {
      evens.add(xs[i]);
    } else {
      odds.add(xs[i]);
    }
  }
  return merge(mergeSort(evens), mergeSort(odds));
}

void main() {
  print(sortedChecksum(readSeed(), mergeSort));
}
