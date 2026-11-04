use std::io::{self, Read};

const MINSTD: i64 = 16807;
const MODULUS: i64 = 2147483647;
const BIG_MOD: i64 = 1000000007;

fn r1(x: i64) -> i64 {
    (x * MINSTD) % MODULUS
}

fn hash_at(s: i64, i: i64) -> i64 {
    r1(r1(s + i))
}

fn read_seed() -> i64 {
    let mut input = String::new();
    let _ = io::stdin().read_to_string(&mut input);
    let m = input
        .lines()
        .next()
        .and_then(|line| line.trim().parse::<i64>().ok())
        .unwrap_or(0);
    if m == 0 {
        1
    } else {
        let mut bytes = [0u8; 8];
        match std::fs::File::open("/dev/urandom") {
            Ok(mut f) => {
                let _ = f.read_exact(&mut bytes);
            }
            Err(_) => {}
        }
        let val = u64::from_le_bytes(bytes);
        (val % 2147483646) as i64 + 1
    }
}

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
