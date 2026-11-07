use std::collections::HashMap;
include!("../../shared/seed.rs");

fn main() {
    let vocab = [
        "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog",
    ];
    let seed = read_seed();

    let mut counts: HashMap<&str, i64> = HashMap::new();
    for i in 0..200000 {
        let word = vocab[(hash_at(seed, i) % 8) as usize];
        *counts.entry(word).or_insert(0) += 1;
    }

    let result: i64 = (0..8)
        .map(|k| (k as i64 + 1) * counts.get(vocab[k]).copied().unwrap_or(0))
        .sum();

    println!("{}", result);
}
