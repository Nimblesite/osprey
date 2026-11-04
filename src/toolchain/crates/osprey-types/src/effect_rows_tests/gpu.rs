use super::{assert_accepted, assert_rejected_with};
use crate::testutil::{accepts, rejects_with};
use osprey_syntax::Flavor;
use std::fmt::Write as _;

// ---------- GPU kernel purity [GPU-KERNEL-PURE], [STAGE-GPU-LEGAL] ----------
// (docs/specs/runtime/0034-GPUComputation.md, docs/specs/language/0017-AlgebraicEffects.md)
//
// Staging generalised the rule from "empty row" to "empty DYNAMIC row", so the
// rejection names what is left to answer at the boundary rather than calling
// the kernel impure: a kernel MAY perform a static effect and still be legal.
// The fail-closed message for an unprovable callback is unchanged
// ([STAGE-GPU-DIAG]).
//
// Fallible GPU producers return `Result` ([FAILURE-EXPLICIT]), so each fixture
// uploads its sources through a `match`: the success payload is the buffer the
// combinator consumes, and every `Error` is reported rather than replaced by a
// placeholder buffer.

#[test]
fn gpu_kernels_must_be_pure_even_under_a_matching_handler() {
    // A handler makes an effect dischargeable on the host; a kernel body still
    // cannot leave the device to reach it, so the map kernel is rejected.
    let steps = [
        ("buffer", "toGpu([1, 2])"),
        ("mapped", "gpuMap(buffer, loud)"),
    ];
    assert_rejected_with(
        &logging_program(
            "fn loud(x) = { perform Log.write(\"saw\")\nx }",
            &steps,
            "print(gpuLength(mapped))",
        ),
        &[DYNAMIC_KERNEL_ERROR],
    );
}

#[test]
fn gpu_fold_combine_kernels_are_purity_checked_too() {
    let steps = [
        ("buffer", "toGpu([1, 2])"),
        ("folded", "gpuFold(buffer, 0, noisyAdd)"),
    ];
    assert_rejected_with(
        &logging_program(
            "fn noisyAdd(acc, x) = { perform Log.write(\"step\")\nwrapAdd(acc, x) }",
            &steps,
            "print(folded)",
        ),
        &[DYNAMIC_KERNEL_ERROR],
    );
}

#[test]
fn unprovable_gpu_kernels_fail_closed() {
    // A kernel received as a bare function parameter has unknown effects at
    // the combinator call site: rejected rather than assumed pure.
    let steps = [("buffer", "toGpu([1])"), ("mapped", "gpuMap(buffer, f)")];
    assert_rejected_with(
        &format!(
            "effect Noise {{ blip: fn() -> Unit }}\nfn runIt(f) = {{\n{}\n}}\nfn main() = runIt(|x| => x)\n",
            gpu_chain(Flavor::Default, &steps, "print(gpuLength(mapped))", 0),
        ),
        &["cannot prove GPU kernel pure"],
    );
}

#[test]
fn pure_gpu_kernels_are_accepted_beside_declared_effects() {
    let steps = [
        ("buffer", "toGpu([1, 2])"),
        ("mapped", "gpuMap(buffer, square)"),
        ("folded", "gpuFold(mapped, 0, |a, x| => wrapAdd(a, x))"),
    ];
    assert_accepted(&logging_program(
        "fn square(x) = wrapMul(x, x)",
        &steps,
        "perform Log.write(\"host ${folded}\")",
    ));
}

/// A Default program whose `Log` handler makes host discharge available around
/// the upload chain, so a kernel's own dynamic effects are the only thing left
/// to diagnose at the combinator.
fn logging_program(kernel: &str, steps: &[(&str, &str)], tail: &str) -> String {
    format!(
        "effect Log {{ write: fn(string) -> Unit }}\n{kernel}\nfn main() = {{\nhandle Log {{\nwrite m => print(m)\n}}\n{}\n}}\n",
        gpu_chain(Flavor::Default, steps, tail, 0),
    )
}

const DYNAMIC_KERNEL_ERROR: &str =
    "kernel body is not stage-legal; it requires dynamic effects: Log.write";

/// A GPU combinator under test: the source list each buffer is uploaded from
/// (bound as `b0`, `b1`, …) and the per-flavor call consuming those buffers.
type Combinator = (
    &'static [&'static str],
    &'static str,
    &'static str,
    &'static str,
);

fn kernel_call(row: &Combinator, flavor: Flavor) -> &'static str {
    match flavor {
        Flavor::Default => row.1,
        Flavor::Ml => row.2,
        Flavor::Python => row.3,
    }
}

/// [STAGE-GPU-LEGAL] Host discharge cannot legalize a dynamic kernel read.
#[test]
fn every_gpu_combinator_checks_transitive_effects_in_all_flavors() {
    for combinator in &COMBINATORS {
        for flavor in [Flavor::Default, Flavor::Ml, Flavor::Python] {
            rejects_with(
                flavor,
                transitive_kernel(flavor, combinator, true),
                DYNAMIC_KERNEL_ERROR,
            );
            accepts(flavor, transitive_kernel(flavor, combinator, false));
        }
    }
}

fn transitive_kernel(flavor: Flavor, combinator: &Combinator, dynamic: bool) -> String {
    let body = kernel_leaf(flavor, dynamic);
    // Python nests the chain inside the handler's `with` suite.
    let indent = match flavor {
        Flavor::Python => 8,
        _ => 4,
    };
    let chain = sources_chain(
        flavor,
        combinator.0,
        kernel_call(combinator, flavor),
        unit_tail(flavor),
        indent,
    );
    match flavor {
        Flavor::Default => {
            format!("effect Log {{ write: fn(string) -> Unit }}\nfn third(x) = {body}\nfn second(x) = third(x)\nfn first(x) = second(x)\nfn main() = {{\n handle Log {{ write m => print(m) }}\n {chain}\n perform Log.write(\"host\")\n}}")
        }
        Flavor::Ml => {
            format!("effect Log\n    write : string => Unit\nthird x = {body}\nsecond x = third x\nfirst x = second x\nmain () =\n    handle Log\n        write m => print m\n{chain}    perform Log.write \"host\"\n")
        }
        Flavor::Python => format!("@effect\nclass Log:\n    def write(message: str) -> None: ...\n@handler(Log)\nclass Logging:\n    def write(message):\n        print(message)\ndef third(x):\n    {body}\ndef second(x):\n    return third(x)\ndef first(x):\n    return second(x)\ndef main():\n    with Logging:\n{chain}        perform(Log.write(\"host\"))\n"),
    }
}

fn kernel_leaf(flavor: Flavor, dynamic: bool) -> &'static str {
    match (flavor, dynamic) {
        (Flavor::Default | Flavor::Ml, false) => "x",
        (Flavor::Python, false) => "return x",
        (Flavor::Default, true) => "{ perform Log.write(\"saw\")\n x }",
        (Flavor::Ml, true) => "\n    perform Log.write \"saw\"\n    x",
        (Flavor::Python, true) => "perform(Log.write(\"saw\"))\n    return x",
    }
}

const COMBINATORS: [Combinator; 5] = [
    (
        &["[1]"],
        "gpuMap(b0, fn(x) => first(x))",
        "gpuMap b0 (\\x => first x)",
        "gpuMap(b0, lambda x: first(x))",
    ),
    (
        &["[1]"],
        "gpuFilter(b0, fn(x) => first(x) > 0)",
        "gpuFilter b0 (\\x => first x > 0)",
        "gpuFilter(b0, lambda x: first(x) > 0)",
    ),
    (
        &["[1]", "[2]"],
        "gpuZipWith(b0, b1, fn(x, y) => wrapAdd(first(x), y))",
        "gpuZipWith (b0, b1, \\(x, y) => wrapAdd (first x) y)",
        "gpuZipWith(b0, b1, lambda x, y: wrapAdd(first(x), y))",
    ),
    (
        &["[1]"],
        "gpuFold(b0, 0, fn(x, y) => wrapAdd(first(x), y))",
        "gpuFold (b0, 0, \\(x, y) => wrapAdd (first x) y)",
        "gpuFold(b0, 0, lambda x, y: wrapAdd(first(x), y))",
    ),
    (
        &["[1]"],
        "gpuScan(b0, 0, fn(x, y) => wrapAdd(first(x), y))",
        "gpuScan (b0, 0, \\(x, y) => wrapAdd (first x) y)",
        "gpuScan(b0, 0, lambda x, y: wrapAdd(first(x), y))",
    ),
];

/// [STAGE-GPU-DIAG] Unknown callbacks fail closed for every combinator.
#[test]
fn every_gpu_combinator_rejects_unprovable_callbacks_in_both_flavors() {
    for combinator in &UNKNOWN_CALLBACKS {
        for flavor in [Flavor::Default, Flavor::Ml, Flavor::Python] {
            rejects_with(
                flavor,
                unknown_callback_program(flavor, combinator),
                "cannot prove GPU kernel pure",
            );
        }
    }
}

fn unknown_callback_program(flavor: Flavor, combinator: &Combinator) -> String {
    let chain = sources_chain(
        flavor,
        combinator.0,
        kernel_call(combinator, flavor),
        unit_tail(flavor),
        4,
    );
    match flavor {
        Flavor::Default => format!("fn run(f) = {{\n{chain}}}\n"),
        Flavor::Ml => format!("run f =\n{chain}"),
        Flavor::Python => format!("def run(f):\n{chain}    return 0\n"),
    }
}

const UNKNOWN_CALLBACKS: [Combinator; 5] = [
    (&["[1]"], "gpuMap(b0, f)", "gpuMap b0 f", "gpuMap(b0, f)"),
    (
        &["[1]"],
        "gpuFilter(b0, f)",
        "gpuFilter b0 f",
        "gpuFilter(b0, f)",
    ),
    (
        &["[1]", "[2]"],
        "gpuZipWith(b0, b1, f)",
        "gpuZipWith (b0, b1, f)",
        "gpuZipWith(b0, b1, f)",
    ),
    (
        &["[1]"],
        "gpuFold(b0, 0, f)",
        "gpuFold (b0, 0, f)",
        "gpuFold(b0, 0, f)",
    ),
    (
        &["[1]"],
        "gpuScan(b0, 0, f)",
        "gpuScan (b0, 0, f)",
        "gpuScan(b0, 0, f)",
    ),
];

/// [ARITH-TOTAL] Extractable builtins still require an explicit host policy.
#[test]
fn scalar_builtin_kernels_cannot_invent_an_arithmetic_policy() {
    for (flavor, sources, call, operation) in [
        (
            Flavor::Default,
            &["[-1]"][..],
            "gpuMap(b0, abs)",
            "Arith.overflow",
        ),
        (Flavor::Ml, &["[-1]"][..], "gpuMap b0 abs", "Arith.overflow"),
        (
            Flavor::Default,
            &["[1]", "[0]"][..],
            "gpuZipWith(b0, b1, intDiv)",
            "Arith.remainderByZero",
        ),
        (
            Flavor::Ml,
            &["[1]", "[0]"][..],
            "gpuZipWith (b0, b1, intDiv)",
            "Arith.remainderByZero",
        ),
    ] {
        let source = scalar_kernel_program(flavor, sources, call);
        rejects_with(
            flavor,
            &source,
            "unhandled effect operations at program entry",
        );
        rejects_with(flavor, &source, operation);
    }
}

fn scalar_kernel_program(flavor: Flavor, sources: &[&str], call: &str) -> String {
    let chain = sources_chain(flavor, sources, call, unit_tail(flavor), 4);
    match flavor {
        Flavor::Default => format!("fn main() = {{\n{chain}}}\n"),
        Flavor::Ml => format!("main () =\n{chain}"),
        Flavor::Python => format!("def main():\n{chain}"),
    }
}

/// [ARITH-EFFECT-DISCHARGE] Every host GPU callback contributes to its caller's row.
#[test]
fn every_gpu_combinator_propagates_arithmetic_and_outer_recovery_requirements() {
    for combinator in &ARITHMETIC_KERNELS {
        for flavor in [Flavor::Default, Flavor::Ml, Flavor::Python] {
            let unhandled = arithmetic_program(flavor, combinator, "");
            rejects_with(
                flavor,
                unhandled,
                "unhandled effect operations at program entry: Arith.overflow",
            );
            accepts(flavor, arithmetic_program(flavor, combinator, "wrapped"));
            rejects_with(
                flavor,
                arithmetic_program(flavor, combinator, "wrapped + 1"),
                "Arith.overflow",
            );
        }
    }
}

fn arithmetic_program(flavor: Flavor, combinator: &Combinator, recovery: &str) -> String {
    let handler = match (flavor, recovery.is_empty()) {
        (_, true) => String::new(),
        (Flavor::Default, false) => {
            format!("handle Arith {{ overflow _ _ _ wrapped => {recovery} }}\n")
        }
        (Flavor::Ml, false) => {
            format!("    handle Arith\n        overflow _ _ _ wrapped => {recovery}\n")
        }
        (Flavor::Python, false) => format!("@handler(Arith)\nclass Policy:\n    def overflow(_op, _left, _right, wrapped):\n        return {recovery}\n"),
    };
    // Python's handled case nests one suite deeper than its unhandled twin.
    let indent = match (flavor, recovery.is_empty()) {
        (Flavor::Python, false) => 8,
        _ => 4,
    };
    let chain = sources_chain(
        flavor,
        combinator.0,
        kernel_call(combinator, flavor),
        unit_tail(flavor),
        indent,
    );
    match flavor {
        Flavor::Default => format!("fn main() = {{ {handler}\n{chain}}}\n"),
        Flavor::Ml => format!("main () =\n{handler}{chain}"),
        Flavor::Python if recovery.is_empty() => format!("def main():\n{chain}"),
        Flavor::Python => format!("{handler}def main():\n    with Policy:\n{chain}"),
    }
}

const ARITHMETIC_KERNELS: [Combinator; 5] = [
    (
        &["[1]"],
        "gpuMap(b0, fn(x) => x + 1)",
        "gpuMap b0 (\\x => x + 1)",
        "gpuMap(b0, lambda x: x + 1)",
    ),
    (
        &["[1]"],
        "gpuFilter(b0, fn(x) => x + 1 > 0)",
        "gpuFilter b0 (\\x => x + 1 > 0)",
        "gpuFilter(b0, lambda x: x + 1 > 0)",
    ),
    (
        &["[1]"],
        "gpuFold(b0, 0, fn(x, y) => x + y)",
        "gpuFold (b0, 0, \\(x, y) => x + y)",
        "gpuFold(b0, 0, lambda x, y: x + y)",
    ),
    (
        &["[1]"],
        "gpuScan(b0, 0, fn(x, y) => x + y)",
        "gpuScan (b0, 0, \\(x, y) => x + y)",
        "gpuScan(b0, 0, lambda x, y: x + y)",
    ),
    (
        &["[1]", "[2]"],
        "gpuZipWith(b0, b1, fn(x, y) => x + y)",
        "gpuZipWith (b0, b1, \\(x, y) => x + y)",
        "gpuZipWith(b0, b1, lambda x, y: x + y)",
    ),
];

// ---------- Fixture builders [FAILURE-EXPLICIT] ----------

/// The fixtures here assert typing and effect rows, not output, so a chain ends
/// in a Unit statement instead of a fabricated value.
fn unit_tail(flavor: Flavor) -> &'static str {
    match flavor {
        Flavor::Default | Flavor::Python => "print(0)",
        Flavor::Ml => "print 0",
    }
}

/// Upload each source into `b0`, `b1`, … then run `call` on those buffers,
/// binding its payload to `mapped` and finishing with `tail`. Default and ML
/// spell a leading `let result = ` at the call site; Python assigns inside the
/// innermost success suite because its `match` is a statement.
fn sources_chain(
    flavor: Flavor,
    sources: &[&str],
    call: &str,
    tail: &str,
    indent: usize,
) -> String {
    let uploads = sources
        .iter()
        .enumerate()
        .map(|(index, source)| (format!("b{index}"), format!("toGpu({source})")))
        .chain([("mapped".to_string(), call.to_string())]);
    let steps: Vec<(String, String)> = uploads.collect();
    gpu_chain(flavor, &steps, tail, indent)
}

/// Nest one `match` per fallible step, binding each success payload to the
/// step's name and reporting every failure; `tail` runs after the last step.
fn gpu_chain<Name: AsRef<str>, Step: AsRef<str>>(
    flavor: Flavor,
    steps: &[(Name, Step)],
    tail: &str,
    indent: usize,
) -> String {
    match flavor {
        Flavor::Default => default_chain(steps, tail),
        Flavor::Ml => ml_chain(steps, tail, indent),
        Flavor::Python => python_chain(steps, tail, indent),
    }
}

/// Default is whitespace-insensitive, so one line per nesting level is enough.
fn default_chain<Name: AsRef<str>, Step: AsRef<str>>(steps: &[(Name, Step)], tail: &str) -> String {
    let mut opened = String::new();
    for (name, step) in steps {
        let _ = write!(
            opened,
            "match {} {{ Success {{ {} }} => {{ ",
            step.as_ref(),
            name.as_ref()
        );
    }
    let closed = " } Error { message } => print(message) }".repeat(steps.len());
    format!("{opened}{tail}{closed}")
}

/// ML nests each `match` in the previous level's success arm, one level of four
/// spaces per upload, so the reported failures close in reverse order.
fn ml_chain<Name: AsRef<str>, Step: AsRef<str>>(
    steps: &[(Name, Step)],
    tail: &str,
    indent: usize,
) -> String {
    let pad = |depth: usize| " ".repeat(indent + 4 * depth);
    let mut out = String::new();
    let mut arm = pad(0);
    for (index, (name, step)) in steps.iter().enumerate() {
        let _ = writeln!(out, "{arm}match {}", step.as_ref());
        arm = format!("{}Success {} => ", pad(index + 1), name.as_ref());
    }
    let _ = writeln!(out, "{arm}{tail}");
    for depth in (1..=steps.len()).rev() {
        let _ = writeln!(out, "{}Error message => print message", pad(depth));
    }
    out
}

/// Python's `match` is a statement: each level contributes a `match`/`case`
/// pair, the payload is assigned in the innermost suite, and two lines per
/// level close the reported failures.
fn python_chain<Name: AsRef<str>, Step: AsRef<str>>(
    steps: &[(Name, Step)],
    tail: &str,
    indent: usize,
) -> String {
    let pad = |depth: usize| " ".repeat(indent + 4 * depth);
    let mut out = String::new();
    for (index, (name, step)) in steps.iter().enumerate() {
        let _ = writeln!(out, "{}match {}:", pad(2 * index), step.as_ref());
        let _ = writeln!(
            out,
            "{}case Success(value={}):",
            pad(2 * index + 1),
            name.as_ref()
        );
    }
    let _ = writeln!(out, "{}{tail}", pad(2 * steps.len()));
    for index in (0..steps.len()).rev() {
        let _ = writeln!(out, "{}case Error(message=message):", pad(2 * index + 1));
        let _ = writeln!(out, "{}print(message)", pad(2 * index + 2));
    }
    out
}
