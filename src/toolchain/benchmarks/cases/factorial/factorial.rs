// Factorial-style product 1*2*...*N taken mod 1000000007 (matches factorial.osp).
const MOD: i64 = 1000000007;

fn main() {
    let mut acc: i64 = 1;
    let mut i: i64 = 1;
    while i <= 10000000 {
        acc = (acc * i) % MOD;
        i += 1;
    }
    println!("{}", acc);
}
