//! Effect-selected execution ([STAGE-BACKEND], docs/specs/language/0017-AlgebraicEffects.md;
//! [GPU-DEVICE], docs/specs/runtime/0034-GPUComputation.md).
//!
//! One model performs a static `Tensor` effect and never names a buffer type.
//! A handler over lists and a handler over GPU buffers each discharge it at
//! compile time, so the emitted IR holds no handler runtime for either, the
//! kernels an arm builds from the model's callbacks compile, and both
//! interpretations print the same doubles in both flavors. The rejections are
//! the two ways the construct could lie: a handler answering a request it was
//! not instantiated for, and a kernel reaching a dynamic effect through a
//! forwarded callback.

#[path = "common/effect_execution.rs"]
mod effect_execution;
#[path = "common/residue.rs"]
mod residue;
#[path = "common/staging.rs"]
mod staging;

use effect_execution::assert_flavored_output;
use osprey_syntax::Flavor;
use residue::HANDLER_RUNTIME_SYMBOLS;
use staging::{compile_for_target, compile_staged, diagnostics};

/// `corner` reads element 3 of a 2x2 product plus element 2 of a doubled iota:
/// 50 + 4 under either interpretation.
const TWO_BACKENDS: &str = r#"
static effect Tensor<B> {
    build: fn(int, fn(int) -> float) -> B
    get: fn(B, int) -> float
    matMul: fn(B, B, int, int, int) -> B
    fromList: fn(List<float>) -> B
}
fn onLists(work) = {
    handle static Tensor<List<float>> {
        build n f => range(0, n) |> map(f) |> toList()
        get b i => listGet(b, i) ?: 0.0
        matMul a b rows inner cols => range(0, rows * cols) |> map(fn(i) => range(0, inner) |> fold(0.0, fn(acc, k) => acc + (listGet(a, intDiv(i, cols) * inner + k) ?: 0.0) * (listGet(b, k * cols + i % cols) ?: 0.0))) |> toList()
        fromList xs => xs
    }
    work()
}
fn onGpu(work) = {
    handle static Tensor<GpuBuffer<float>> {
        build n f => toGpu(range(0, n)) |> gpuMap(f)
        get b i => gpuGet(b, i) ?: 0.0
        matMul a b rows inner cols => gpuMatMul(a, b, rows, inner, cols) ?: toGpu([])
        fromList xs => toGpu(xs)
    }
    work()
}
fn at(m, i) = perform Tensor.get(m, i)
fn vector(xs) = perform Tensor.fromList(xs)
fn doubled(n) = perform Tensor.build(n, fn(i) => toFloat(i) * 2.0)
fn corner() = at(perform Tensor.matMul(vector([1.0, 2.0, 3.0, 4.0]), vector([5.0, 6.0, 7.0, 8.0]), 2, 2, 2), 3) + at(doubled(3), 2)
fn main() = {
    handle Arith {
        overflow _ _ _ wrapped => wrapped
        divideByZero _ _ => 0.0
        remainderByZero _ => 0
    }
    print("${onLists(corner)} ${onGpu(corner)}")
}
"#;

const TWO_BACKENDS_ML: &str = r#"
static effect Tensor B
    build : (int, (int) -> float) => B
    get : (B, int) => float
    matMul : (B, B, int, int, int) => B
    fromList : List<float> => B
onLists work =
    handle static Tensor<List<float>>
        build n f => range 0 n |> map f |> toList
        get b i => listGet b i ?: 0.0
        matMul a b rows inner cols => range 0 (rows * cols) |> map (\i => range 0 inner |> fold 0.0 (\(acc, k) => acc + (listGet a (intDiv i cols * inner + k) ?: 0.0) * (listGet b (k * cols + i % cols) ?: 0.0))) |> toList
        fromList xs => xs
    work ()
onGpu work =
    handle static Tensor<GpuBuffer<float>>
        build n f => toGpu (range (0, n)) |> gpuMap f
        get b i => gpuGet b i ?: 0.0
        matMul a b rows inner cols => gpuMatMul a b rows inner cols ?: toGpu []
        fromList xs => toGpu xs
    work ()
at (m, i) = perform Tensor.get m i
vector xs = perform Tensor.fromList xs
doubled n = perform Tensor.build n (\i => toFloat i * 2.0)
corner () = at (perform Tensor.matMul (vector [1.0, 2.0, 3.0, 4.0]) (vector [5.0, 6.0, 7.0, 8.0]) 2 2 2, 3) + at (doubled 3, 2)
main () =
    handle Arith
        overflow _ _ _ wrapped => wrapped
        divideByZero _ _ => 0.0
        remainderByZero _ => 0
    print "${onLists corner} ${onGpu corner}"
"#;

const BOTH_ANSWERS: &str = "54.0 54.0\n";

/// A record whose fields are the effect's `B` is one type per interpretation,
/// so a program running both carries two instantiations of every helper that
/// touches it — including the `?:` fallback that builds an empty one.
const RECORDS_OVER_B: &str = r#"
static effect Tensor<B> {
    fromList: fn(List<float>) -> B
    get: fn(B, int) -> float
}
type Pair<B> = { left: B, right: B }
fn onLists(work) = {
    handle static Tensor<List<float>> {
        fromList xs => xs
        get b i => listGet(b, i) ?: 0.0
    }
    work()
}
fn onGpu(work) = {
    handle static Tensor<GpuBuffer<float>> {
        fromList xs => toGpu(xs)
        get b i => gpuGet(b, i) ?: 0.0
    }
    work()
}
fn pair(k) = Pair { left: perform Tensor.fromList([1.0, 2.0]), right: perform Tensor.fromList([k]) }
fn emptyPair() = {
    let e = perform Tensor.fromList([])
    Pair { left: e, right: e }
}
fn sum(p) = perform Tensor.get(p.left, 1) + perform Tensor.get(p.right, 0)
fn run() = {
    let pairs = range(0, 3) |> fold([], fn(acc, i) => listAppend(acc, pair(toFloat(i))))
    sum(listGet(pairs, 2) ?: emptyPair()) + sum(listGet(pairs, 7) ?: emptyPair())
}
fn main() = {
    handle Arith {
        overflow _ _ _ wrapped => wrapped
        divideByZero _ _ => 0.0
        remainderByZero _ => 0
    }
    print("${onLists(run)} ${onGpu(run)}")
}
"#;

/// A `Stash<string>` region does not answer a request inference instantiates
/// at `int` ([EFFECTS-GENERIC-INSTANTIATION]): the mismatch is a rejection,
/// never a silent coercion.
const MISMATCHED_INSTANTIATION: &str = r#"
static effect Stash<T> { put: fn(T) -> int }
fn storeNumber() = perform Stash.put(42)
let done = {
    handle static Stash<string> { put value => length(value) }
    storeNumber()
}
print("${done}")
"#;

/// A static arm may forward the model's callback into a kernel, so the callback
/// is judged where it lands: one that performs a dynamic effect is rejected by
/// the row it would need, exactly as an inline kernel is ([GPU-KERNEL-PURE]).
const FORWARDED_DYNAMIC_KERNEL: &str = r#"
static effect Build { build: fn(int, fn(int) -> float) -> GpuBuffer<float> }
effect Log { write: fn(string) -> Unit }
fn loud(i) = {
    perform Log.write("saw ${i}")
    toFloat(i)
}
fn twice(n) = perform Build.build(n, loud)
let out = {
    handle Log { write m => print(m) }
    handle static Build { build n f => toGpu(range(0, n)) |> gpuMap(f) }
    twice(3)
}
print("${gpuGet(out, 2) ?: 0.0}")
"#;

const FORWARDED_DYNAMIC_KERNEL_ML: &str = r#"
static effect Build
    build : (int, (int) -> float) => GpuBuffer<float>
effect Log
    write : string => Unit
loud i =
    perform Log.write "saw ${i}"
    toFloat i
twice n = perform Build.build n loud
out =
    handle Log
        write m => print m
    handle static Build
        build n f => toGpu (range (0, n)) |> gpuMap f
    twice 3
print "${gpuGet out 2 ?: 0.0}"
"#;

const KERNEL_REJECTION: &str =
    "kernel body is not stage-legal; it requires dynamic effects: Log.write";

#[test]
fn both_interpretations_are_discharged_before_the_program_runs() {
    let ir = compile_staged(TWO_BACKENDS);
    // The only handler runtime left is the dynamic `Arith` policy `main`
    // installs: registering it names the effect, so a `Tensor` registration
    // would name that effect too, and the IR never mentions it.
    assert!(
        HANDLER_RUNTIME_SYMBOLS
            .iter()
            .any(|symbol| ir.contains(symbol)),
        "the dynamic Arith policy must still register a handler"
    );
    assert!(
        !ir.contains("Tensor"),
        "a static Tensor region left its effect in the IR"
    );
    // The GPU arms reach the buffer runtime; the list arms never do.
    assert!(
        ir.contains("@osprey_gpu_matmul("),
        "the GPU product was not emitted"
    );
    assert!(
        ir.contains("@osprey_gpu_get("),
        "the GPU gather was not emitted"
    );
}

#[test]
fn both_interpretations_compute_the_same_doubles_in_both_flavors() {
    assert_flavored_output("tensor_backends", "osp", TWO_BACKENDS, BOTH_ANSWERS);
    assert_flavored_output("tensor_backends", "ospml", TWO_BACKENDS_ML, BOTH_ANSWERS);
}

#[test]
fn records_over_the_effects_representation_serve_both_interpretations() {
    assert_flavored_output("records_over_b", "osp", RECORDS_OVER_B, "4.0 4.0\n");
}

#[test]
fn a_discharged_tensor_program_compiles_for_wasm32() {
    // Static discharge leaves no dynamic control, so the program is admissible
    // on the target that rejects it ([STAGE-WASM]).
    let (ok, stderr) = compile_for_target(TWO_BACKENDS, "wasm32");
    assert!(ok, "wasm32 compile failed: {stderr}");
}

#[test]
fn an_explicit_instantiation_never_answers_another_instantiation() {
    let report = diagnostics(MISMATCHED_INSTANTIATION, Flavor::Default);
    assert!(
        report.contains("Stash<int>.put"),
        "expected the int request to stay unhandled, got: {report}"
    );
}

#[test]
fn a_forwarded_kernel_whose_callback_performs_a_dynamic_effect_is_rejected() {
    for (source, flavor) in [
        (FORWARDED_DYNAMIC_KERNEL, Flavor::Default),
        (FORWARDED_DYNAMIC_KERNEL_ML, Flavor::Ml),
    ] {
        let report = diagnostics(source, flavor);
        assert!(
            report.contains(KERNEL_REJECTION),
            "{flavor:?}: expected `{KERNEL_REJECTION}`, got: {report}"
        );
    }
}

/// A helper that selects the interpretation by name and forwards the work to
/// its installer, as `src/examples/gpt2` selects `BACKEND`: the selected handler
/// answers the forwarded requests, from a named function or a lambda
/// ([STAGE-LOWER-ORDER]). Static lowering once left them unanswered and pruned
/// `main`, so the program printed nothing and exited 0.
#[test]
fn a_handler_selected_through_a_helper_answers_the_forwarded_work() {
    let source = TWO_BACKENDS.replace(
        r#"print("${onLists(corner)} ${onGpu(corner)}")"#,
        r#"print("${under("lists", corner)} ${under("gpu", fn() => corner())}")"#,
    ) + "fn under(name, work) = match name {\n    \"lists\" => onLists(work)\n    _ => onGpu(work)\n}\n";
    assert_flavored_output("selected_backend", "osp", &source, BOTH_ANSWERS);
}

/// Each arm hands a helper the callback it calls: a named function and an
/// inline lambda, neither performing anything.
const KNOWN_CALLBACKS: &str = r#"
static effect Measure {
    named: fn(string) -> int
    inline: fn(string) -> int
}
fn apply(f, x) = f(x)
fn letters(s) = length(s)
fn main() = {
    handle static Measure {
        named s => apply(letters, s)
        inline s => apply(fn(t) => letters(t), s)
    }
    print("${perform Measure.named("abc")} ${perform Measure.inline("de")}")
}
"#;

/// The same helper handed a callback that reaches a dynamic effect.
const DYNAMIC_CALLBACK: &str = r#"
effect Log {
    write: fn(string) -> Unit
}
static effect Measure {
    named: fn(string) -> int
}
fn apply(f, x) = f(x)
fn loud(s) = {
    perform Log.write(s)
    length(s)
}
fn main() = {
    handle Log {
        write m => print(m)
    }
    handle static Measure {
        named s => apply(loud, s)
    }
    print("${perform Measure.named("abc")}")
}
"#;

/// [STAGE-STATIC-MONOTONE] A static arm may reach operations through helpers
/// and callbacks. Handing a helper a named function or an inline lambda tells
/// the checker exactly what the helper calls, as it does for a GPU kernel
/// ([GPU-KERNEL-PURE]), so these arms introduce nothing and must discharge.
/// Static lowering marked every helper that calls a parameter `<unknown>`
/// without looking at the call site, so GPT-2's two handlers could not share
/// one `orAbort` that takes each handler's own abort.
#[test]
fn a_static_arm_may_hand_a_helper_a_known_callback() {
    assert_flavored_output("known_callbacks", "osp", KNOWN_CALLBACKS, "3 2\n");
}

/// The precision must not cost soundness: the callback's dynamic operation is
/// named, not hidden behind `<unknown>` and not admitted.
#[test]
fn a_static_arm_handing_a_helper_a_dynamic_callback_names_its_effect() {
    let report = diagnostics(DYNAMIC_CALLBACK, Flavor::Default);
    assert!(
        report.contains("static handler arm `Measure.named` requires runtime effects: Log.write;"),
        "expected the arm to name Log.write, got: {report}"
    );
}
