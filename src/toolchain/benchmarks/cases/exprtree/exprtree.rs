const BIG_MOD: i64 = 1000000007;
include!("../../shared/seed.rs");

enum Expr {
    Lit(i64),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
}

fn build(s: i64, idx: i64, depth: i64) -> Expr {
    if depth == 0 {
        Expr::Lit(hash_at(s, idx) % 1000)
    } else {
        let op = hash_at(s, idx) % 2;
        let l = build(s, idx * 2 + 1, depth - 1);
        let r = build(s, idx * 2 + 2, depth - 1);
        if op == 0 {
            Expr::Add(Box::new(l), Box::new(r))
        } else {
            Expr::Mul(Box::new(l), Box::new(r))
        }
    }
}

fn eval(e: &Expr) -> i64 {
    match e {
        Expr::Lit(v) => *v,
        Expr::Add(l, r) => (eval(l) + eval(r)) % BIG_MOD,
        Expr::Mul(l, r) => (eval(l) * eval(r)) % BIG_MOD,
    }
}

fn main() {
    let seed = read_seed();

    let mut acc: i64 = 0;
    for t in 0..10 {
        acc = (acc + eval(&build(seed + t * 7, 1, 14))) % BIG_MOD;
    }

    println!("{}", acc);
}
