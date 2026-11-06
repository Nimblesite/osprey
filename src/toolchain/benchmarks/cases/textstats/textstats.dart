// Text statistics over a tiny vocab — exercises length/char-search per word.
import 'dart:io';
import 'dart:math';

const int minstd = 16807;
const int modulus = 2147483647;

int r1(int x) => (x * minstd) % modulus;

int hashAt(int s, int i) => r1(r1(s + i));

bool containsChar(String w, int code) {
  for (int j = 0; j < w.length; j++) {
    if (w.codeUnitAt(j) == code) {
      return true;
    }
  }
  return false;
}

int readSeed() {
  final line = stdin.readLineSync();
  int m = 0;
  if (line != null) {
    int end = 0;
    while (end < line.length &&
        (line.codeUnitAt(end) == 0x20 || line.codeUnitAt(end) == 0x09)) {
      end++;
    }
    final start = end;
    if (end < line.length &&
        (line.codeUnitAt(end) == 0x2B || line.codeUnitAt(end) == 0x2D)) {
      end++;
    }
    while (end < line.length &&
        line.codeUnitAt(end) >= 0x30 &&
        line.codeUnitAt(end) <= 0x39) {
      end++;
    }
    if (end > start) {
      m = int.tryParse(line.substring(start, end)) ?? 0;
    }
  }
  if (m == 0) {
    return 1;
  }
  final rng = Random.secure();
  final val = (rng.nextInt(1 << 32) << 32) | rng.nextInt(1 << 32);
  return val.toUnsigned(63) % 2147483646 + 1;
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
