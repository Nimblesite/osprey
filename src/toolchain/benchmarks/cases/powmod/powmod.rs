// Sum of modular exponentiation: sum of i^20 mod P for i in 1..N, naive repeated multiply.
const P: i64 = 1000000007;

fn powmod(base: i64, e: i64, acc: i64) -> i64 {
    match e {
        0 => acc,
        _ => powmod(base, e - 1, (acc * base) % P),
    }
}

fn main() {
    let mut acc: i64 = 0;
    let mut i: i64 = 1;
    while i < 1000000 {
        acc = (acc + powmod(i, 20, 1)) % P;
        i += 1;
    }
    println!("{}", acc);
}
