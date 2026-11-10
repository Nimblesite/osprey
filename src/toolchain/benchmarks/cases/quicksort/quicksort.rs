include!("../../shared/seed.rs");
include!("../../shared/sorted.rs");

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
    println!("{}", sorted_checksum(read_seed(), quicksort));
}
