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
