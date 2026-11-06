// Allocate + traverse many short-lived binary trees — allocation/memory stress.
// Faithful port of binarytrees.osp: build 1200 trees of depth 13, sum checks.

enum Tree {
    Leaf,
    Node { left: Box<Tree>, right: Box<Tree> },
}

fn make(d: i64) -> Tree {
    if d == 0 {
        Tree::Node {
            left: Box::new(Tree::Leaf),
            right: Box::new(Tree::Leaf),
        }
    } else {
        Tree::Node {
            left: Box::new(make(d - 1)),
            right: Box::new(make(d - 1)),
        }
    }
}

fn check(t: &Tree) -> i64 {
    match t {
        Tree::Leaf => 0,
        Tree::Node { left, right } => 1 + check(left) + check(right),
    }
}

fn main() {
    let mut acc: i64 = 0;
    let mut i: i64 = 0;
    while i < 1200 {
        acc += check(&make(13));
        i += 1;
    }
    println!("{}", acc);
}
