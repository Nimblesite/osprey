// Text statistics over a tiny vocab — exercises length/char-search per word.

import '../../shared/seed.dart';

bool containsChar(String w, int code) {
  for (int j = 0; j < w.length; j++) {
    if (w.codeUnitAt(j) == code) {
      return true;
    }
  }
  return false;
}

void main() {
  const vocab = ['the', 'quick', 'brown', 'fox', 'jumps', 'over', 'lazy', 'dog'];
  final seed = readSeed();

  int acc = 0;
  for (int i = 0; i < 200000; i++) {
    final w = vocab[hashAt(seed, i) % 8];
    acc += w.length +
        (containsChar(w, 0x6F) ? 7 : 0) +
        (w.codeUnitAt(0) == 0x74 ? 3 : 0);
  }

  print(acc);
}
