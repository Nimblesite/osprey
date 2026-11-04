// Count coprime pairs (i,j), 1<=i,j<=N — nested iteration + Euclidean gcd.
fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn main() {
    let n: i64 = 2000;
    let mut acc: i64 = 0;
    let mut i: i64 = n;
    while i > 0 {
        let mut j: i64 = n;
        while j > 0 {
            if gcd(i, j) == 1 {
                acc += 1;
            }
            j -= 1;
        }
        i -= 1;
    }
    println!("{}", acc);
}
