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

fn main() {
    let seed = read_seed();

    let mut acc: i64 = 0;
    for t in 0..8 {
        let s = seed + t * 131;
        let xs: Vec<i64> = (0..4000).map(|i| hash_at(s, i) % 1000).collect();
        let sum: i64 = xs.iter().sum();
        let below: i64 = xs.iter().filter(|&&x| x < 500).count() as i64;
        acc = (acc + sum + below) % BIG_MOD;
    }

    println!("{}", acc);
}
