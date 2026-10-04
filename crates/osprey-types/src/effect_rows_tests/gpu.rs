use super::{assert_accepted, assert_rejected_with};
use crate::testutil::{accepts, rejects_with};
use osprey_syntax::Flavor;

// ---------- GPU kernel purity [GPU-KERNEL-PURE], [STAGE-GPU-LEGAL] ----------
// (docs/specs/0034-GPUComputation.md, docs/specs/0035-StagedEffects.md)
//
// Staging generalised the rule from "empty row" to "empty DYNAMIC row", so the
// rejection names what is left to answer at the boundary rather than calling
// the kernel impure: a kernel MAY perform a static effect and still be legal.
// The fail-closed message for an unprovable callback is unchanged
// ([STAGE-GPU-DIAG]).

#[test]
fn gpu_kernels_must_be_pure_even_under_a_matching_handler() {
    // A handler makes an effect dischargeable on the host; a kernel body still
    // cannot leave the device to reach it, so the map kernel is rejected.
    assert_rejected_with(
        "effect Log { write: fn(string) -> Unit }\n\
         fn loud(x) = {\n\
         perform Log.write(\"saw\")\n\
         x\n\
         }\n\
         fn main() = {\n\
         let n = {\n\
             handle Log {\n\
                 write m => print(m)\n\
             }\n\
             toGpu([1, 2]) |> gpuMap(loud) |> gpuLength()\n\
         }\n\
         print(n)\n\
         }\n",
        &["kernel body is not stage-legal; it requires dynamic effects: Log.write"],
    );
}

#[test]
fn gpu_fold_combine_kernels_are_purity_checked_too() {
    assert_rejected_with(
        "effect Log { write: fn(string) -> Unit }\n\
         fn noisyAdd(acc, x) = {\n\
         perform Log.write(\"step\")\n\
         wrapAdd(acc, x)\n\
         }\n\
         fn main() = {\n\
         let n = {\n\
             handle Log {\n\
                 write m => print(m)\n\
             }\n\
             toGpu([1, 2]) |> gpuFold(0, noisyAdd)\n\
         }\n\
         print(n)\n\
         }\n",
        &["kernel body is not stage-legal; it requires dynamic effects: Log.write"],
    );
}

#[test]
fn unprovable_gpu_kernels_fail_closed() {
    // A kernel received as a bare function parameter has unknown effects at
    // the combinator call site: rejected rather than assumed pure.
    assert_rejected_with(
        "effect Noise { blip: fn() -> Unit }\n\
         fn runIt(f) = toGpu([1]) |> gpuMap(f) |> gpuLength()\n\
         fn main() = print(runIt(|x| => x))\n",
        &["cannot prove GPU kernel pure"],
    );
}

#[test]
fn pure_gpu_kernels_are_accepted_beside_declared_effects() {
    assert_accepted(
        "effect Log { write: fn(string) -> Unit }\n\
         fn square(x) = wrapMul(x, x)\n\
         fn main() = {\n\
         let total = toGpu([1, 2]) |> gpuMap(square) |> gpuFold(0, |a, x| => wrapAdd(a, x))\n\
         handle Log {\n\
             write m => print(m)\n\
         }\n\
         perform Log.write(\"host ${total}\")\n\
         }\n",
    );
}

const DYNAMIC_KERNEL_ERROR: &str =
    "kernel body is not stage-legal; it requires dynamic effects: Log.write";

/// [STAGE-GPU-LEGAL] Host discharge cannot legalize a dynamic kernel read.
#[test]
fn every_gpu_combinator_checks_transitive_effects_in_both_flavors() {
    for (default, ml) in COMBINATORS {
        for (expression, flavor) in [(default, Flavor::Default), (ml, Flavor::Ml)] {
            rejects_with(
                flavor,
                transitive_kernel(flavor, expression, true),
                DYNAMIC_KERNEL_ERROR,
            );
            accepts(flavor, transitive_kernel(flavor, expression, false));
        }
    }
}

fn transitive_kernel(flavor: Flavor, expression: &str, dynamic: bool) -> String {
    let body = kernel_leaf(flavor, dynamic);
    match flavor {
        Flavor::Default => {
            format!("effect Log {{ write: fn(string) -> Unit }}\nfn third(x) = {body}\nfn second(x) = third(x)\nfn first(x) = second(x)\nfn main() = {{\n handle Log {{ write m => print(m) }}\n let result = {expression}\n perform Log.write(\"host\")\n}}")
        }
        Flavor::Ml => {
            format!("effect Log\n    write : string => Unit\nthird x = {body}\nsecond x = third x\nfirst x = second x\nmain () =\n    handle Log\n        write m => print m\n    result = {expression}\n    perform Log.write \"host\"\n")
        }
    }
}

fn kernel_leaf(flavor: Flavor, dynamic: bool) -> &'static str {
    match (flavor, dynamic) {
        (_, false) => "x",
        (Flavor::Default, true) => "{ perform Log.write(\"saw\")\n x }",
        (Flavor::Ml, true) => "\n    perform Log.write \"saw\"\n    x",
    }
}

const COMBINATORS: [(&str, &str); 5] = [
    (
        "gpuMap(toGpu([1]), fn(x) => first(x))",
        "gpuMap (toGpu [1]) (\\x => first x)",
    ),
    (
        "gpuFilter(toGpu([1]), fn(x) => first(x) > 0)",
        "gpuFilter (toGpu [1]) (\\x => first x > 0)",
    ),
    (
        "gpuZipWith(toGpu([1]), toGpu([2]), fn(x, y) => wrapAdd(first(x), y))",
        "gpuZipWith (toGpu [1], toGpu [2], \\(x, y) => wrapAdd (first x) y)",
    ),
    (
        "gpuFold(toGpu([1]), 0, fn(x, y) => wrapAdd(first(x), y))",
        "gpuFold (toGpu [1], 0, \\(x, y) => wrapAdd (first x) y)",
    ),
    (
        "gpuScan(toGpu([1]), 0, fn(x, y) => wrapAdd(first(x), y))",
        "gpuScan (toGpu [1], 0, \\(x, y) => wrapAdd (first x) y)",
    ),
];

/// [STAGE-GPU-DIAG] Unknown callbacks fail closed for every combinator.
#[test]
fn every_gpu_combinator_rejects_unprovable_callbacks_in_both_flavors() {
    for (default, ml) in UNKNOWN_CALLBACKS {
        rejects_with(
            Flavor::Default,
            format!("fn run(f) = {{\n let result = {default}\n 0\n }}"),
            "cannot prove GPU kernel pure",
        );
        rejects_with(
            Flavor::Ml,
            format!("run f =\n    result = {ml}\n    0"),
            "cannot prove GPU kernel pure",
        );
    }
}

const UNKNOWN_CALLBACKS: [(&str, &str); 5] = [
    ("gpuMap(toGpu([1]), f)", "gpuMap (toGpu [1]) f"),
    ("gpuFilter(toGpu([1]), f)", "gpuFilter (toGpu [1]) f"),
    (
        "gpuZipWith(toGpu([1]), toGpu([2]), f)",
        "gpuZipWith (toGpu [1], toGpu [2], f)",
    ),
    ("gpuFold(toGpu([1]), 0, f)", "gpuFold (toGpu [1], 0, f)"),
    ("gpuScan(toGpu([1]), 0, f)", "gpuScan (toGpu [1], 0, f)"),
];

/// [ARITH-TOTAL] Extractable builtins still require an explicit host policy.
#[test]
fn scalar_builtin_kernels_cannot_invent_an_arithmetic_policy() {
    for (default, ml, operation) in [
        (
            "gpuMap(toGpu([-1]), abs)",
            "gpuMap (toGpu [-1]) abs",
            "Arith.overflow",
        ),
        (
            "gpuZipWith(toGpu([1]), toGpu([0]), intDiv)",
            "gpuZipWith (toGpu [1], toGpu [0], intDiv)",
            "Arith.remainderByZero",
        ),
    ] {
        for (source, flavor) in [
            (format!("fn main() = gpuLength({default})"), Flavor::Default),
            (format!("main () = gpuLength ({ml})"), Flavor::Ml),
        ] {
            rejects_with(
                flavor,
                &source,
                "unhandled effect operations at program entry",
            );
            rejects_with(flavor, &source, operation);
        }
    }
}

/// [ARITH-EFFECT-DISCHARGE] Every host GPU callback contributes to its caller's row.
#[test]
fn every_gpu_combinator_propagates_arithmetic_and_outer_recovery_requirements() {
    for (default, ml) in ARITHMETIC_KERNELS {
        for (body, flavor) in [(default, Flavor::Default), (ml, Flavor::Ml)] {
            let unhandled = arithmetic_program(flavor, body, "");
            rejects_with(
                flavor,
                unhandled,
                "unhandled effect operations at program entry: Arith.overflow",
            );
            accepts(flavor, arithmetic_program(flavor, body, "wrapped"));
            rejects_with(
                flavor,
                arithmetic_program(flavor, body, "wrapped + 1"),
                "Arith.overflow",
            );
        }
    }
}

fn arithmetic_program(flavor: Flavor, body: &str, recovery: &str) -> String {
    let handler = match (flavor, recovery.is_empty()) {
        (_, true) => String::new(),
        (Flavor::Default, false) => {
            format!("handle Arith {{ overflow _ _ _ wrapped => {recovery} }}\n")
        }
        (Flavor::Ml, false) => {
            format!("    handle Arith\n        overflow _ _ _ wrapped => {recovery}\n")
        }
    };
    match flavor {
        Flavor::Default => format!("fn main() = {{ {handler} let result = {body}\n print(0) }}"),
        Flavor::Ml => format!("main () =\n{handler}    result = {body}\n    print 0"),
    }
}

const ARITHMETIC_KERNELS: [(&str, &str); 5] = [
    (
        "gpuMap(toGpu([1]), fn(x) => x + 1)",
        "gpuMap (toGpu [1]) (\\x => x + 1)",
    ),
    (
        "gpuFilter(toGpu([1]), fn(x) => x + 1 > 0)",
        "gpuFilter (toGpu [1]) (\\x => x + 1 > 0)",
    ),
    (
        "gpuFold(toGpu([1]), 0, fn(x, y) => x + y)",
        "gpuFold (toGpu [1], 0, \\(x, y) => x + y)",
    ),
    (
        "gpuScan(toGpu([1]), 0, fn(x, y) => x + y)",
        "gpuScan (toGpu [1], 0, \\(x, y) => x + y)",
    ),
    (
        "gpuZipWith(toGpu([1]), toGpu([2]), fn(x, y) => x + y)",
        "gpuZipWith (toGpu [1], toGpu [2], \\(x, y) => x + y)",
    ),
];
