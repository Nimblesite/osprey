// Allocate + traverse many short-lived binary trees — allocation/memory stress: build 1200 trees of depth 13, sum checks.

class Tree {
  left: Tree | null = null;
  right: Tree | null = null;
}

function make(d: number): Tree {
  const t = new Tree();
  if (d !== 0) {
    t.left = make(d - 1);
    t.right = make(d - 1);
  }
  return t;
}

function check(t: Tree | null): number {
  if (t === null) return 0;
  return 1 + check(t.left) + check(t.right);
}

function main(): void {
  let acc = 0;
  for (let i = 0; i < 1200; i++) {
    const t = make(13);
    acc += check(t);
  }
  console.log(acc);
}

main();
