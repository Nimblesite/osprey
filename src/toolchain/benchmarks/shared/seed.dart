// The seed and hash the list and text cases share, imported by each case so
// `dart compile exe` still builds one case file into one binary.
import 'dart:io';
import 'dart:math';

const int minstd = 16807;
const int modulus = 2147483647;

int r1(int x) => (x * minstd) % modulus;

int hashAt(int s, int i) => r1(r1(s + i));

// Reads the seed's leading integer as C's strtoll does: 0 selects the fixed
// seed 1, anything else a random one.
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

// The sort cases' shared run: eight seeded runs of 2000 values below 100000,
// each sorted by `sort` and folded into one rank-weighted checksum.
int sortedChecksum(int seed, List<int> Function(List<int>) sort) {
  const bigMod = 1000000007;
  int acc = 0;
  for (int t = 0; t < 8; t++) {
    final s = seed + t * 131;
    final sorted = sort(List<int>.generate(2000, (i) => hashAt(s, i) % 100000));
    int weight = 0;
    for (int i = 0; i < sorted.length; i++) {
      weight = (weight + sorted[i] * (i + 1)) % bigMod;
    }
    acc = (acc + weight) % bigMod;
  }
  return acc;
}
