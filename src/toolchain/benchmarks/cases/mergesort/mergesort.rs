include!("../../shared/seed.rs");
include!("../../shared/sorted.rs");

fn merge(left: &[i64], right: &[i64]) -> Vec<i64> {
    let mut merged = Vec::with_capacity(left.len() + right.len());
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        if right[j] < left[i] {
            merged.push(right[j]);
            j += 1;
        } else {
            merged.push(left[i]);
            i += 1;
        }
    }
    merged.extend_from_slice(&left[i..]);
    merged.extend_from_slice(&right[j..]);
    merged
}

// Naive top-down merge sort: deal the slice into two fresh halves by
// alternating elements, sort both, merge.
fn merge_sort(xs: &[i64]) -> Vec<i64> {
    if xs.len() < 2 {
        return xs.to_vec();
    }
    let evens: Vec<i64> = xs.iter().copied().step_by(2).collect();
    let odds: Vec<i64> = xs.iter().copied().skip(1).step_by(2).collect();
    merge(&merge_sort(&evens), &merge_sort(&odds))
}

fn main() {
    println!("{}", sorted_checksum(read_seed(), merge_sort));
}
