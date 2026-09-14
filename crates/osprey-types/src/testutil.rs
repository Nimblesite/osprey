//! Shared `#[cfg(test)]` helpers for the type-checker unit tests.
//!
//! Every test module parses a snippet (asserting it is syntactically valid),
//! runs [`check_program`](crate::check::check_program), and asserts on the
//! resulting diagnostics. These helpers hoist that boilerplate so each module
//! re-uses one canonical copy via `use crate::testutil::*;`.

use crate::check::check_program;
use crate::error::TypeError;
use osprey_syntax::{parse_program_with_flavor, Flavor};

/// Parse under `flavor` + type-check, returning the diagnostics. Panics if the
/// snippet has syntax errors, since a program that never reaches the checker
/// must not be scored as "no type errors".
pub(crate) fn typecheck(flavor: Flavor, src: &str) -> Vec<TypeError> {
    let parsed = parse_program_with_flavor(src, flavor);
    assert!(
        parsed.errors.is_empty(),
        "{flavor} flavor rejected the snippet at parse time: {:?}\n{src}",
        parsed.errors
    );
    check_program(&parsed.program)
}

/// Parse + type-check a Default-flavor snippet, returning the diagnostics.
pub(crate) fn check(src: &str) -> Vec<TypeError> {
    typecheck(Flavor::Default, src)
}

/// Parse + type-check, asserting the snippet is well-typed (no diagnostics).
pub(crate) fn ok(src: &str) {
    accepts(Flavor::Default, src);
}

/// Parse + type-check, asserting at least one type error is reported.
pub(crate) fn bad(src: &str) -> Vec<TypeError> {
    let errs = check(src);
    assert!(!errs.is_empty(), "expected a type error, got none");
    errs
}

/// Assert `src` is accepted whole under `flavor`: parsed AND well-typed.
pub(crate) fn accepts(flavor: Flavor, src: impl AsRef<str>) {
    let src = src.as_ref();
    let errs = typecheck(flavor, src);
    assert!(
        errs.is_empty(),
        "{flavor}: unexpected type errors: {errs:?}\n{src}"
    );
}

/// Assert the CHECKER rejects `src` with a diagnostic containing `needle`. A
/// rejection for any other reason — a syntax error included — fails the test,
/// so a feature that is merely unparseable can never masquerade as checked.
pub(crate) fn rejects_with(flavor: Flavor, src: impl AsRef<str>, needle: impl AsRef<str>) {
    let (src, needle) = (src.as_ref(), needle.as_ref());
    let errs = typecheck(flavor, src);
    assert!(
        errs.iter().any(|e| e.message.contains(needle)),
        "{flavor}: expected a diagnostic containing {needle:?}, got {errs:?}\n{src}"
    );
}

/// Assert a Default-flavor snippet is rejected with a diagnostic containing
/// `needle` — the `bad(…)` plus `errs.iter().any(…)` pair these modules would
/// otherwise repeat once per case.
pub(crate) fn bad_with(src: &str, needle: &str) {
    rejects_with(Flavor::Default, src, needle);
}

/// Assert the CHECKER rejects `src`, whatever the wording. Used where the claim
/// is that a relation does NOT hold, and pinning one sentence would over-specify
/// which of several true diagnostics must be the one reported.
pub(crate) fn rejects(flavor: Flavor, src: impl AsRef<str>) {
    let src = src.as_ref();
    let errs = typecheck(flavor, src);
    assert!(
        !errs.is_empty(),
        "{flavor}: expected a type error, got none\n{src}"
    );
}

/// Assert `src` is rejected SOMEHOW — by the parser or by the checker. Used
/// where the spec forbids a form without fixing which layer must catch it.
pub(crate) fn rejected_somehow(flavor: Flavor, src: impl AsRef<str>) {
    let src = src.as_ref();
    let parsed = parse_program_with_flavor(src, flavor);
    let rejected = !parsed.errors.is_empty() || !check_program(&parsed.program).is_empty();
    assert!(rejected, "{flavor}: expected a rejection, got none\n{src}");
}

/// The call-site arity contract's exact wording, mirroring
/// [GENERICS-CTOR-ARITY]'s ``constructor `Box` takes 1 type argument(s), got 2``.
pub(crate) fn fn_arity_message(name: &str, declared: usize, written: usize) -> String {
    format!("function `{name}` takes {declared} type argument(s), got {written}")
}

/// The construction-site arity wording ([GENERICS-CTOR-ARITY]).
pub(crate) fn ctor_arity_message(name: &str, declared: usize, written: usize) -> String {
    format!("constructor `{name}` takes {declared} type argument(s), got {written}")
}

/// The declaration-site variance diagnostic, from
/// `examples/failscompilation/variance_out_in_input_position.ospo`. `kind` is
/// `field` for a type declaration and `operation` for an effect declaration
/// ([TYPE-VARIANCE-POSITIONS], [EFFECTS-GENERIC-DECL]).
pub(crate) fn variance_position_message(
    param: &str,
    marker: &str,
    position: &str,
    kind: &str,
    name: &str,
    owner: &str,
) -> String {
    format!(
        "type parameter `{param}` is declared `{marker} {param}` but appears in \
         {position} position in {kind} `{name}` of `{owner}`"
    )
}

/// One `#[test]` per spec sentence, without one hand-written wrapper per
/// sentence. Every row keeps its own name, its own doc comment and its own
/// test binary entry — only the `#[test] fn … { verb(Flavor::…, …) }` scaffold
/// is written once here instead of once per case. `verb` is any assertion
/// above, so a row reads as the claim it pins:
///
/// ```ignore
/// spec_cases! {
///     /// A written argument pins the callee's binder.
///     pins_a_binder: accepts(Default, format!("{IDENTITY}print(\"${{identity<int>(5)}}\")"));
///     /// The swapped twin must not check.
///     swapped_is_rejected: rejects_with(Default, SWAPPED, "cannot unify");
/// }
/// ```
macro_rules! spec_cases {
    ($(
        $(#[$meta:meta])*
        $name:ident: $verb:ident($flavor:ident, $($arg:expr),+ $(,)?);
    )*) => {
        $(
            $(#[$meta])*
            #[test]
            fn $name() {
                crate::testutil::$verb(osprey_syntax::Flavor::$flavor, $($arg),+);
            }
        )*
    };
}
pub(crate) use spec_cases;

/// The same collapse for assertions that take no flavor — a module's own
/// helpers, say. A row is one or more calls, so a case that pins a claim in
/// both directions keeps both assertions in one test:
///
/// ```ignore
/// plain_cases! {
///     /// `List<out T>` accepts its own element type.
///     list_accepts_the_identical_element: flows(BUILTINS, "List<int>", "listInt()");
///     /// …and refuses the coercion either way.
///     list_refuses_the_coercion: blocked(B, "List<Result<int, E>>", "listInt()"),
///                                blocked(B, "List<int>", "listRes()");
/// }
/// ```
macro_rules! plain_cases {
    ($(
        $(#[$meta:meta])*
        $name:ident: $($verb:ident($($arg:expr),* $(,)?)),+ ;
    )*) => {
        $(
            $(#[$meta])*
            #[test]
            fn $name() {
                $($verb($($arg),*);)+
            }
        )*
    };
}
pub(crate) use plain_cases;
