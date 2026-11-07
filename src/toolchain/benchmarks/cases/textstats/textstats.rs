include!("../../shared/seed.rs");

fn main() {
    let vocab = [
        "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog",
    ];
    let seed = read_seed();

    let mut acc: i64 = 0;
    for i in 0..200000 {
        let w = vocab[(hash_at(seed, i) % 8) as usize];
        acc += w.len() as i64
            + if w.contains('o') { 7 } else { 0 }
            + if w.starts_with('t') { 3 } else { 0 };
    }

    println!("{}", acc);
}
