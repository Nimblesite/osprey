//! Expression inference tests: pipes, iterators, simple forms and select.

use crate::testutil::{bad, bad_with, ok};

#[test]
fn pipe_into_call_and_bare_function() {
    // Call form: `x |> f(a)` prepends `x`. Bare form: `x |> f` applies `f(x)`.
    ok("fn add(a, b) = a + b\n\
        fn inc(n) = n + 1\n\
        fn main() = {\n\
          handle Arith { overflow _ _ _ wrapped => wrapped }\n\
          let r = 10 |> add(5)\n\
          let s = 10 |> inc\n\
        }\n");
}

#[test]
fn fused_iterator_functions_reject_materialized_lists() {
    ok("fn main() = {\n\
          handle Arith { overflow _ _ _ wrapped => wrapped }\n\
          range(0, 3) |> map(fn(x) => x + 1) |> forEach(print)\n\
        }\n");
    let errs = bad("[1, 2, 3] |> map(fn(x) => x + 1) |> forEach(print)\n");
    assert!(errs
        .iter()
        .any(|e| e.message.contains("Iterator") && e.message.contains("List")));
}

#[test]
fn covers_every_simple_expression_form() {
    // The parser only emits many `Expr` arms from real source, so one program
    // mixes float/string/bool/interpolation/list/map/object/unary/field-access/
    // index/lambda/block/spawn/await/channel send+recv/yield/perform.
    ok("type Box = { v: int }\n\
        effect Logger { log: fn(string) -> Unit }\n\
        fn other() -> int = 7\n\
        fn demo() -> Unit !Logger = {\n\
          let f = 3.14\n\
          let s = \"hi\"\n\
          let b = true\n\
          let count = 5\n\
          let i = \"val=${count}\"\n\
          let xs = [1, 2, 3]\n\
          let m = { \"a\": 1, \"b\": 2 }\n\
          let obj = { x: 1, y: 2 }\n\
          let neg = -5\n\
          let no = !b\n\
          let bx = Box { v: 9 }\n\
          let fx = bx.v\n\
          let first = xs[0]\n\
          let g = fn(n) => n + 1\n\
          let fib = spawn other()\n\
          let r = await(fib)\n\
          let ch = Channel(1)\n\
          send(ch, 42)\n\
          let got = recv(ch)\n\
          yield\n\
          perform Logger.log(\"hello\")\n\
        }\n");
}

#[test]
fn select_types_each_operation_and_joins_its_arms() {
    // Implements [CONCURRENCY-SELECT].
    ok(
        "fn pick(a: Channel<int>, b: Channel<string>) -> int = select {\n\
          x = recv(a) => x\n\
          y = recv(b) => length(y)\n\
          send(a, 1) => 2\n\
          timeout(5) => 3\n\
        }\n",
    );
    for (arm, message) in [
        ("y = recv(a) => length(y)", "`length` supports only"),
        ("send(a, \"text\") => 1", "cannot unify int with string"),
        ("recv(a) => \"text\"", "cannot unify"),
    ] {
        bad_with(
            &format!("fn pick(a: Channel<int>) -> int = select {{\n {arm}\n _ => 0\n}}\n"),
            message,
        );
    }
    bad_with(
        "fn pick(a: Channel<int>) -> int = select {\n recv(a) => 1\n timeout(\"soon\") => 2\n}\n",
        "cannot unify int with string",
    );
    bad_with(
        "fn pick() -> int = select {\n timeout(1) => 2\n}\n",
        "needs at least one `send` or `recv` arm",
    );
    bad_with(
        "fn pick(a: Channel<int>) -> int = select {\n recv(a) => 1\n timeout(1) => 2\n _ => 3\n}\n",
        "at most one `timeout` or `_` arm",
    );
}
