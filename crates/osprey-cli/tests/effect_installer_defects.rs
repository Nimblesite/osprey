//! RED tests pinning effect-installer defects found on 2026-08-22.
//!
//! These drive the real binary because every one of them is a LOWERING or LINK
//! failure: the frontend accepts each program, so an in-process type-check
//! oracle cannot see any of them. Two produce silently wrong output — the
//! program exits ZERO having printed a raw pointer where a string belonged —
//! which is the single worst outcome the language can produce, and the reason
//! these are pinned red rather than described in a comment.
//!
//! Do not weaken an assertion here to make one pass. See
//! `tests/modules/README.md` for the full write-up.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Write `body` to a uniquely-named temp source and run it through `--run`,
/// answering the exit code and the merged transcript.
fn run_source(name: &str, extension: &str, body: &str) -> (Option<i32>, String) {
    let path = std::env::temp_dir().join(format!("osprey_installer_defect_{name}.{extension}"));
    // A failed write surfaces as a parse/read failure the assertions catch.
    let _ = std::fs::write(&path, body);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_osprey"));
    let _ = cmd
        .current_dir(repo_root())
        .arg(&path)
        .arg("--run")
        .arg("--quiet");
    match cmd.output() {
        Ok(out) => (
            out.status.code(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        ),
        Err(e) => (None, format!("spawn failed: {e}")),
    }
}

/// One generic handler installer, applied at two different body result types.
/// [TYPE-GENERICS-FN] makes `feeding` polymorphic in what its body answers, and
/// [EFFECTS-HANDLER] puts no restriction on that, so both call sites must
/// render their own value.
///
/// DEFECT: evaluated directly inside a top-level statement's interpolation, the
/// two instantiations collide. The `string` site renders the RAW POINTER as an
/// integer and the run still exits ZERO — a different, machine-dependent number
/// on every execution, so nothing downstream can even notice. Reversing the two
/// statements turns the same collision into a SIGSEGV instead. Binding each call
/// to a `let` first, or making the calls from inside a function body, avoids it.
#[test]
fn a_generic_handler_installer_renders_both_instantiations() {
    let source = concat!(
        "effect Feed { next : fn() -> int }\n\n",
        "fn feeding(reading, body) = handle Feed\n",
        "    next => resume(reading)\n",
        "in body()\n\n",
        "print(\"${feeding(9, fn() => \"done\")}\")\n",
        "print(\"${feeding(3, fn() => perform Feed.next())}\")\n",
    );
    let (code, transcript) = run_source("polymorphic_installer", "osp", source);
    assert_eq!(code, Some(0), "run did not complete: {transcript}");
    assert_eq!(
        transcript, "done\n3\n",
        "a polymorphic installer rendered the wrong value"
    );
}

/// The same program with the statements swapped. [FLAVOR-IR-EQUIV] and ordinary
/// evaluation order both make this the same two calls in the other sequence.
///
/// DEFECT: this ordering crashes outright (SIGSEGV, exit 139) rather than
/// printing a wrong number. Same root cause, louder symptom — pinned separately
/// so a fix that only silences one ordering cannot pass.
#[test]
fn a_generic_handler_installer_survives_either_statement_order() {
    let source = concat!(
        "effect Feed { next : fn() -> int }\n\n",
        "fn feeding(reading, body) = handle Feed\n",
        "    next => resume(reading)\n",
        "in body()\n\n",
        "print(\"${feeding(3, fn() => perform Feed.next())}\")\n",
        "print(\"${feeding(9, fn() => \"done\")}\")\n",
    );
    let (code, transcript) = run_source("polymorphic_installer_swapped", "osp", source);
    assert_eq!(code, Some(0), "run did not complete: {transcript}");
    assert_eq!(
        transcript, "3\ndone\n",
        "a polymorphic installer rendered the wrong value"
    );
}

/// [FLAVOR-ML-CURRY]: whitespace parameters curry, and [FLAVOR-IR-EQUIV] makes
/// ML's `feeding reading body` the same function as Default's
/// `fn feeding(reading, body)`.
///
/// DEFECT: when the curried spine's trailing parameter is the body a `handle`
/// installs over, the emitted IR references an undefined `_body` symbol from the
/// resume trampoline and the link fails. The tupled ML head
/// `feeding (reading, body)` — which lowers to a flat parameter list — compiles,
/// so the defect is in how currying interacts with resume-body outlining.
#[test]
fn a_curried_ml_installer_emits_a_linkable_resume_trampoline() {
    let source = concat!(
        "effect Feed\n    next : Unit => int\n\n",
        "feeding reading body =\n",
        "    handle Feed\n",
        "        next => resume reading\n",
        "    in body ()\n\n",
        "print \"${feeding 3 (\\() => perform Feed.next ())}\"\n",
    );
    let (code, transcript) = run_source("curried_ml_installer", "ospml", source);
    assert_eq!(code, Some(0), "run did not complete: {transcript}");
    assert_eq!(
        transcript, "3\n",
        "curried ML installer printed the wrong value"
    );
}

/// The example the ML flavor spec itself prints under [FLAVOR-ML-BIND]
/// (docs/specs/0024-MLFlavorSyntax.md): a single-line handler arm that assigns
/// to a handler-owned cell, `tick => requests := (requests + 1) ?: requests`.
///
/// DEFECT: the ML parser rejects `:=` in a single-line arm body; only the
/// indented-block form parses. The spec's own example does not compile, so
/// either the grammar or the spec is stale — and until they agree, a reader
/// following the documentation writes a program the compiler refuses.
#[test]
fn a_single_line_ml_handler_arm_may_assign_to_its_cell() {
    let source = concat!(
        "effect Counter\n    tick : Unit => Unit\n\n",
        "mut requests = 0\n",
        "total = handle Counter\n",
        "    tick => requests := (requests + 1) ?: requests\n",
        "in perform Counter.tick ()\n\n",
        "print \"${requests}\"\n",
    );
    let (code, transcript) = run_source("ml_single_line_arm_assign", "ospml", source);
    assert_eq!(code, Some(0), "run did not complete: {transcript}");
    assert_eq!(transcript, "1\n", "the spec's own arm form did not run");
}

/// [MODULES-FILE-SCOPE-BINDING] sanctions BOTH a file-scope handler whose arms
/// own a `mut` cell AND a file-scope binding naming a generic function, and its
/// two worked examples sit in the same section of the same spec page.
///
/// DEFECT: a source carrying both is rejected — "program entry invokes a dynamic
/// callable whose effect provenance cannot be proven" — although each half
/// compiles and runs on its own. The generic binding has no runtime value to
/// store ([TYPE-GENERICS-FN]), so it cannot be the dynamic callable the entry
/// invokes; the effect-provenance check is over-approximating.
#[test]
fn a_file_scope_handler_and_a_generic_binding_coexist() {
    let source = concat!(
        "effect Counter { tick : fn(int) -> int }\n\n",
        "fn run() = perform Counter.tick(3)\n\n",
        "let total = handle Counter\n    tick amount => amount\nin run()\n\n",
        "fn identity(x) = x\n",
        "let alias = identity\n\n",
        "fn round(n) = alias(n)\n\n",
        "print(\"${total} ${round(1)}\")\n",
    );
    let (code, transcript) = run_source("handler_beside_generic_binding", "osp", source);
    assert_eq!(code, Some(0), "run did not complete: {transcript}");
    assert_eq!(
        transcript, "3 1\n",
        "the two file-scope forms did not coexist"
    );
}

/// [EFFECTS-HANDLER-STATE] promotes a file-scope `mut` to a cell a handler arm
/// may read, and Osprey's Hindley-Milner inference means a helper whose types
/// are inferable carries no annotation. Each half is ordinary Osprey, and the
/// pair is the natural spelling of the retry handler in [MULTI-FALSIFY] case 1
/// (docs/specs/0035-StagedEffects.md, plan 0028).
///
/// DEFECT: reading such a cell as the ARGUMENT of an unannotated helper whose
/// result is a `Result` is rejected at lowering with "`attempts` has no
/// resolved signature". That message comes from `named_fn_cell`
/// (crates/osprey-codegen/src/closure.rs), which emits a forwarder for a
/// top-level FUNCTION used as a value — so codegen resolved the mutable binding
/// as a named function. Three variants of the same program compile and print
/// `0`: annotating the helper `fn settle(attempt: int) -> Result<int, string>`,
/// making the binding a `let`, or moving the call out of the arm. Nothing about
/// the program is genuinely unresolved, and the diagnostic names a signature the
/// author never wrote.
///
/// A helper returning a non-`Result` type from the same arm compiles, which is
/// what makes this a `Result`-shaped lowering defect rather than a rule about
/// mutable capture.
#[test]
fn a_handler_arm_reads_a_mut_cell_into_an_inferred_result_helper() {
    let source = concat!(
        "effect Charge { charge : fn(int) -> int }\n\n",
        "fn settle(attempt) = match attempt {\n",
        "    1 => Error { message: \"declined\" }\n",
        "    _ => Success { value: 7 }\n",
        "}\n\n",
        "mut attempts = 1\n",
        "let outcome = handle Charge\n",
        "    charge amount => match settle(attempts) {\n",
        "        Success { value } => value\n",
        "        Error { message } => 0\n",
        "    }\n",
        "in perform Charge.charge(1)\n\n",
        "print(\"${outcome}\")\n",
    );
    let (code, transcript) = run_source("arm_mut_cell_into_result_helper", "osp", source);
    assert_eq!(code, Some(0), "run did not complete: {transcript}");
    let path =
        std::env::temp_dir().join("osprey_installer_defect_arm_mut_cell_into_result_helper.osp");
    assert_eq!(
        transcript, format!(
            "0\n\n{}\n  10:4  warning: unused handler parameter `amount` of `Charge.charge`\n  10:4  warning: unused pattern binding `message`\n\n2 warnings (unused-handler-parameter, unused-pattern-binding)\n",
            path.display()
        ),
        "the arm did not read the promoted mutable cell"
    );
}
