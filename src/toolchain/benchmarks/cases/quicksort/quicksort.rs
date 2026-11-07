const BIG_MOD: i64 = 1000000007;
include!("../../shared/seed.rs");

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
