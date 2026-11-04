// Allocate + traverse many short-lived binary trees — allocation/memory stress: build 1200 trees of depth 13, sum checks.

class Tree {
  Tree? left;
  Tree? right;
}

Tree make(int d) {
  final t = Tree();
  if (d != 0) {
    t.left = make(d - 1);
    t.right = make(d - 1);
  }
  return t;
}

int check(Tree? t) {
  if (t == null) return 0;
  return 1 + check(t.left) + check(t.right);
}

void main() {
  int acc = 0;
  for (int i = 0; i < 1200; i++) {
    final t = make(13);
    acc += check(t);
  }
  print(acc);
}
