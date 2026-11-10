// The seed and hash the list and text cases share, imported by each case so
// `dart compile exe` still builds one case file into one binary.
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
    m = int.tryParse(line.trim()) ?? 0;
  }
  if (m == 0) {
    return 1;
  }
  final rng = Random.secure();
  final val = (rng.nextInt(1 << 31) << 31) | rng.nextInt(1 << 31);
  return (val % 2147483646) + 1;
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
