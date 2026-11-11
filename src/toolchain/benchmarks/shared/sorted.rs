// The sort cases' shared run, read through include! after seed.rs: eight
// seeded runs of 2000 values below 100000, each sorted by `sort` and folded
// into one rank-weighted checksum.
const BIG_MOD: i64 = 1000000007;

fn sorted_checksum(seed: i64, sort: fn(&[i64]) -> Vec<i64>) -> i64 {
    (0..8).fold(0, |acc, t| {
        let s = seed + t * 131;
        let xs: Vec<i64> = (0..2000).map(|i| hash_at(s, i) % 100000).collect();
        let weight = sort(&xs)
            .iter()
            .zip(1..)
            .fold(0, |total, (&x, rank)| (total + x * rank) % BIG_MOD);
        (acc + weight) % BIG_MOD
    })
}
