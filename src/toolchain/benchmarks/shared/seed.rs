// The seed and hash every list and text case shares: read through include!,
// so each case still builds as one rustc invocation with no crate.
use std::io::{self, Read};

const MINSTD: i64 = 16807;
const MODULUS: i64 = 2147483647;

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
