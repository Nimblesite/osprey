use super::{run_file, temp_osp, Out};

const EFFECT_RETURN_MISMATCH: &str = r"
namespace app;
signature Api {
    effect Fx {
        tick: fn() -> int
    }
}
module M : Api {
    effect Fx {
        tick: fn() -> string
    }
}
fn main() = 0
";

#[test]
fn signature_effect_return_type_mismatch_is_rejected() {
    assert_mismatch("signature_effect_return_mismatch", EFFECT_RETURN_MISMATCH);
}

fn check(name: &str, source: &str) -> Out {
    let path = temp_osp(name, source);
    let out = run_file(&path, &["--check", "--memory=gc"]);
    let _ = std::fs::remove_file(&path);
    out
}

fn assert_mismatch(name: &str, source: &str) {
    let out = check(name, source);
    assert_eq!(
        out.code,
        Some(1),
        "stdout: {}; stderr: {}",
        out.stdout,
        out.stderr
    );
    assert!(
        out.stderr
            .contains("effect `Fx` operations do not match its signature"),
        "{}",
        out.stderr
    );
}

#[test]
fn signature_effect_parameter_type_and_mode_mismatches_are_rejected() {
    for (actual, expected) in [
        ("tick: fn(string) -> int", "tick: fn(int) -> int"),
        ("control tick: fn() -> int", "tick: fn() -> int"),
    ] {
        let source = EFFECT_RETURN_MISMATCH
            .replace("tick: fn() -> string", actual)
            .replace("tick: fn() -> int", expected);
        assert_mismatch("signature_effect_mismatch", &source);
    }
}

#[test]
fn signature_matching_effect_is_accepted() {
    let source = EFFECT_RETURN_MISMATCH.replace("fn() -> string", "fn() -> int");
    let out = check("signature_effect_matching", &source);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
}

#[test]
fn signature_effect_binder_renaming_preserves_concrete_type_names() {
    let source = r"
namespace app;
type Tree<T> = { value: T }
signature Api { effect Fx<T> { read: fn(Tree<T>) -> T } }
module M : Api { effect Fx<U> { read: fn(Tree<U>) -> U } }
fn main() = 0
";
    let out = check("signature_effect_binder_renaming", source);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
}
