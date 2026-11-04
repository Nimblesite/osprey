// Triple-nested counting loop accumulating (i*j*k) mod P — nested iteration.
const P: i64 = 1000000007;

fn loop_k(i: i64, j: i64, mut k: i64, mut acc: i64) -> i64 {
    while k != 0 {
        acc = (acc + i * j * k) % P;
        k -= 1;
    }
    acc
}

fn loop_j(i: i64, mut j: i64, n: i64, mut acc: i64) -> i64 {
    while j != 0 {
        acc = loop_k(i, j, n, acc);
        j -= 1;
    }
    acc
}

fn loop_i(mut i: i64, n: i64, mut acc: i64) -> i64 {
    while i != 0 {
        acc = loop_j(i, n, n, acc);
        i -= 1;
    }
    acc
}

fn main() {
    println!("{}", loop_i(250, 250, 0));
}
