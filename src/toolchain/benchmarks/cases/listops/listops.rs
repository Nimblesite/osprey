const BIG_MOD: i64 = 1000000007;
include!("../../shared/seed.rs");

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
