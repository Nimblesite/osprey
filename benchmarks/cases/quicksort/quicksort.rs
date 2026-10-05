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

// Naive first-element-pivot quicksort: split the tail into two fresh vectors
// around the pivot, sort both, join.
fn quicksort(xs: &[i64]) -> Vec<i64> {
    match xs.split_first() {
        None => Vec::new(),
        Some((&pivot, rest)) => {
            let below: Vec<i64> = rest.iter().copied().filter(|&x| x < pivot).collect();
            let at_least: Vec<i64> = rest.iter().copied().filter(|&x| x >= pivot).collect();
            let mut sorted = quicksort(&below);
            sorted.push(pivot);
            sorted.extend(quicksort(&at_least));
            sorted
        }
    }
}

fn main() {
    let seed = read_seed();

    let mut acc: i64 = 0;
    for t in 0..8 {
        let s = seed + t * 131;
        let xs: Vec<i64> = (0..2000).map(|i| hash_at(s, i) % 100000).collect();
        let sorted = quicksort(&xs);
        let weight = sorted
            .iter()
            .zip(1..)
            .fold(0, |total, (&x, rank)| (total + x * rank) % BIG_MOD);
        acc = (acc + weight) % BIG_MOD;
    }

    println!("{}", acc);
}
