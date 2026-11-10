// Word-frequency count over a tiny vocab — exercises hashing + counting.

import '../../shared/seed.dart';

void main() {
  final seed = readSeed();

  final counts = List<int>.filled(8, 0);
  for (int i = 0; i < 200000; i++) {
    counts[hashAt(seed, i) % 8] += 1;
  }

  int result = 0;
  for (int k = 0; k < 8; k++) {
    result += (k + 1) * counts[k];
  }

  print(result);
}
