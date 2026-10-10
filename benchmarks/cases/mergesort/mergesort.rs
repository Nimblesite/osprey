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

fn merge(left: &[i64], right: &[i64]) -> Vec<i64> {
    let mut merged = Vec::with_capacity(left.len() + right.len());
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        if right[j] < left[i] {
            merged.push(right[j]);
            j += 1;
        } else {
            merged.push(left[i]);
            i += 1;
        }
    }
    merged.extend_from_slice(&left[i..]);
    merged.extend_from_slice(&right[j..]);
    merged
}

// Naive top-down merge sort: deal the slice into two fresh halves by
// alternating elements, sort both, merge.
fn merge_sort(xs: &[i64]) -> Vec<i64> {
    if xs.len() < 2 {
        return xs.to_vec();
    }
    let evens: Vec<i64> = xs.iter().copied().step_by(2).collect();
    let odds: Vec<i64> = xs.iter().copied().skip(1).step_by(2).collect();
    merge(&merge_sort(&evens), &merge_sort(&odds))
}

fn main() {
    let seed = read_seed();

    let mut acc: i64 = 0;
    for t in 0..8 {
        let s = seed + t * 131;
        let xs: Vec<i64> = (0..2000).map(|i| hash_at(s, i) % 100000).collect();
        let sorted = merge_sort(&xs);
        let weight = sorted
            .iter()
            .zip(1..)
            .fold(0, |total, (&x, rank)| (total + x * rank) % BIG_MOD);
        acc = (acc + weight) % BIG_MOD;
    }

    println!("{}", acc);
}
