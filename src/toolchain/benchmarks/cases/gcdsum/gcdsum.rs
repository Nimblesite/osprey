// Sum of gcd(i, K) for i in 1..N — Euclidean-algorithm recursion (modulo heavy).
fn gcd(a: i64, b: i64) -> i64 {
    match b {
        0 => a,
        _ => gcd(b, a % b),
    }
}

fn main() {
    let mut acc: i64 = 0;
    let mut i: i64 = 1;
    while i < 2000000 {
        acc += gcd(i, 1234567);
        i += 1;
    }
    println!("{}", acc);
}
