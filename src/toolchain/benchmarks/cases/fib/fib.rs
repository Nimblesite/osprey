// Naive recursive Fibonacci — exercises raw function-call + recursion overhead.
fn add(a: i64, b: i64) -> i64 {
    a + b
}

fn sub(a: i64, b: i64) -> i64 {
    a - b
}

fn fib(n: i64) -> i64 {
    match n {
        0 => 0,
        1 => 1,
        _ => add(fib(sub(n, 1)), fib(sub(n, 2))),
    }
}

fn main() {
    println!("{}", fib(35));
}
