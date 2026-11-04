// Expression tree — build a heap-allocated tree, then evaluate it.
import 'dart:io';
import 'dart:math';

const int minstd = 16807;
const int modulus = 2147483647;
const int bigMod = 1000000007;

const int tagLit = 0;
const int tagAdd = 1;
const int tagMul = 2;

int r1(int x) => (x * minstd) % modulus;

int hashAt(int s, int i) => r1(r1(s + i));

int readSeed() {
  int m = 0;
  final line = stdin.readLineSync();
  if (line != null) {
    m = int.tryParse(line.trim()) ?? 0;
  }
  if (m == 0) {
    return 1;
  }
  return Random.secure().nextInt(2147483646) + 1;
}

class Expr {
  int tag = tagLit;
  int v = 0;
  Expr? l;
  Expr? r;
}

Expr build(int s, int idx, int depth) {
  final e = Expr();
  if (depth == 0) {
    e.tag = tagLit;
    e.v = hashAt(s, idx) % 1000;
  } else {
    final op = hashAt(s, idx) % 2;
    e.v = 0;
    e.l = build(s, idx * 2 + 1, depth - 1);
    e.r = build(s, idx * 2 + 2, depth - 1);
    e.tag = op == 0 ? tagAdd : tagMul;
  }
  return e;
}

int eval(Expr e) {
  switch (e.tag) {
    case tagLit:
      return e.v;
    case tagAdd:
      return (eval(e.l!) + eval(e.r!)) % bigMod;
    default:
      return (eval(e.l!) * eval(e.r!)) % bigMod;
  }
}

void main() {
  final seed = readSeed();

  int acc = 0;
  for (int t = 0; t < 10; t++) {
    acc = (acc + eval(build(seed + t * 7, 1, 14))) % bigMod;
  }

  print(acc);
}
