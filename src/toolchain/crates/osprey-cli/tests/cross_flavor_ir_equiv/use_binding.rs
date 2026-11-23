//! `use` is sugar and nothing else ([BLOCK-USE]): a block of `use` steps lowers
//! to the same canonical AST as the hand-written pyramid of calls and lambdas it
//! replaces, so the emitted LLVM IR is byte-identical, in both flavors.
use super::{ir_for, Flavor};

const STEP: &str = "fn step(i, x) = match x < 100 {\n    true => Success { value: x + i }\n    false => Error { message: \"step ${toString(i)} refused\" }\n}\nfn scoped(label, body) = \"<${label}>${body()}</${label}>\"\nfn both(x, y, next) = next(x, y)\n";

const ML_STEP: &str = "step (i, x) = match x < 100\n    true => Success(value = x + i)\n    false => Error(message = \"step ${toString i} refused\")\nscoped (label, body) = \"<${label}>${body ()}</${label}>\"\nboth (x, y, next) = next (x, y)\n";

/// Each case: a `use` program, its flavor, and the Default pyramid it must equal.
const CASES: [(&str, &str, Flavor, &str); 6] = [
    (
        "three binds",
        "fn chain(n) = {\n    use a <- andThen(step(1, n))\n    use b <- andThen(step(2, a))\n    use c <- andThen(step(3, b))\n    Success { value: a + b + c }\n}\n",
        Flavor::Default,
        "fn chain(n) = andThen(step(1, n), fn(a) => andThen(step(2, a), fn(b) => andThen(step(3, b), fn(c) => Success { value: a + b + c })))\n",
    ),
    (
        "guards",
        "fn chain(n) = {\n    use _ <- andThen(step(1, n))\n    use b <- andThen(step(2, n))\n    Success { value: b }\n}\n",
        Flavor::Default,
        "fn chain(n) = andThen(step(1, n), fn(_) => andThen(step(2, n), fn(b) => Success { value: b }))\n",
    ),
    (
        "binder-less and two binders",
        "fn chain(n) = {\n    use <- scoped(\"outer\")\n    use p, q <- both(n, n + 1)\n    \"${toString(p * q)}\"\n}\n",
        Flavor::Default,
        "fn chain(n) = scoped(\"outer\", fn() => both(n, n + 1, fn(p, q) => \"${toString(p * q)}\"))\n",
    ),
    (
        "ML three binds",
        "chain n =\n    use a <- andThen (step (1, n))\n    use b <- andThen (step (2, a))\n    use c <- andThen (step (3, b))\n    Success(value = a + b + c)\n",
        Flavor::Ml,
        "fn chain(n) = andThen(step(1, n), fn(a) => andThen(step(2, a), fn(b) => andThen(step(3, b), fn(c) => Success { value: a + b + c })))\n",
    ),
    (
        "ML guards",
        "chain n =\n    use _ <- andThen (step (1, n))\n    use b <- andThen (step (2, n))\n    Success(value = b)\n",
        Flavor::Ml,
        "fn chain(n) = andThen(step(1, n), fn(_) => andThen(step(2, n), fn(b) => Success { value: b }))\n",
    ),
    (
        "ML binder-less and two binders",
        "chain n =\n    use <- scoped (\"outer\")\n    use p, q <- both (n, n + 1)\n    \"${toString (p * q)}\"\n",
        Flavor::Ml,
        "fn chain(n) = scoped(\"outer\", fn() => both(n, n + 1, fn(p, q) => \"${toString(p * q)}\"))\n",
    ),
];

const DEFAULT_MAIN: &str = "print(\"${toString(chain(4))}\")\n";
const ML_MAIN: &str = "print \"${toString (chain 4)}\"\n";

/// [BLOCK-USE] Every `use` spelling emits exactly the IR of its pyramid.
#[test]
fn use_blocks_emit_the_ir_of_the_pyramid_they_replace() -> Result<(), String> {
    let mut drift = Vec::new();
    for (name, use_body, flavor, pyramid_body) in CASES {
        let (prelude, main) = match flavor {
            Flavor::Ml => (ML_STEP, ML_MAIN),
            _ => (STEP, DEFAULT_MAIN),
        };
        let flat = ir_for(&format!("{prelude}{use_body}{main}"), flavor, name)?;
        let pyramid = ir_for(
            &format!("{STEP}{pyramid_body}{DEFAULT_MAIN}"),
            Flavor::Default,
            name,
        )?;
        if flat != pyramid {
            drift.push(format!("{name}: {}", super::first_diff(&pyramid, &flat)));
        }
    }
    assert!(drift.is_empty(), "{}", drift.join("\n"));
    Ok(())
}
