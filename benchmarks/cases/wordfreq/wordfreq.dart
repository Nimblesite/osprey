// Word-frequency count over a tiny vocab — exercises hashing + counting.
import 'dart:io';
import 'dart:math';

const int minstd = 16807;
const int modulus = 2147483647;

int r1(int x) => (x * minstd) % modulus;

int hashAt(int s, int i) => r1(r1(s + i));

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
