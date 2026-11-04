// Binomial coefficient via naive (un-memoised) Pascal recursion C(n,k)=C(n-1,k-1)+C(n-1,k).
fn binom(n: i64, k: i64) -> i64 {
    if k == 0 {
        1
    } else if k == n {
        1
    } else {
        binom(n - 1, k - 1) + binom(n - 1, k)
    }
}

fn main() {
    println!("{}", binom(27, 13));
}
