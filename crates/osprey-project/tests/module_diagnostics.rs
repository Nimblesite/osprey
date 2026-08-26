//! Source-level rejection coverage for [MODULES-DIAG].
//!
//! `modules.rs` and `state_modules.rs` drive most of these branches from
//! hand-built AST, which proves the checker rejects a *shape*. This file drives
//! them from **written source in both flavors**, which is the only way to prove
//! the surface a user actually types reaches that checker at all — a lowering
//! that dropped an `export` marker or a `mut` would leave every AST-built test
//! green while the language silently accepted the program.
//!
//! Each case is asserted on BOTH surfaces with the SAME expected message, so a
//! diagnostic that regresses on one flavor cannot hide behind the other
//! ([FLAVOR-IR-EQUIV], [MODULES-FLAVOR-PROJECTION]).

mod support;

use osprey_project::SourceFile;
use osprey_syntax::{parse_program_with_flavor, Flavor};
use std::path::PathBuf;
use support::{config, contains, error_messages};

/// Every diagnostic one written source produces, the way the CLI collects them:
/// parse errors first — some boundary rules (ML's exact ascription) are decided
/// by the grammar and never reach assembly — otherwise the assembler's.
fn diagnose(flavor: Flavor, text: &str) -> Vec<String> {
    let name = match flavor {
        Flavor::Ml => "main.ospml",
        Flavor::Default => "main.osp",
    };
    let parsed = parse_program_with_flavor(text, flavor);
    if !parsed.errors.is_empty() {
        return parsed.errors.into_iter().map(|e| e.message).collect();
    }
    let source = SourceFile {
        path: PathBuf::from(name),
        flavor,
        source: text.to_string(),
        program: parsed.program,
    };
    error_messages(&config(name), &[source])
}

/// Assert one expected substring against the Default and ML spellings of the
/// same program. Both surfaces lower to one AST, so both must report it.
fn both_reject(expected: &str, default_source: &str, ml_source: &str) {
    let default = diagnose(Flavor::Default, default_source);
    assert!(
        contains(&default, expected),
        "Default surface lost {expected:?}: {default:?}"
    );
    let ml = diagnose(Flavor::Ml, ml_source);
    assert!(
        contains(&ml, expected),
        "ML surface lost {expected:?}: {ml:?}"
    );
}

#[test]
fn exported_mutable_cell_is_rejected_on_both_surfaces() {
    // Implements [MODULES-STATE-TOPLEVEL]: `export mut` is always an error.
    // The two surfaces decide it at different stages — ML's grammar refuses the
    // marker outright, Default's assembler refuses the cell — so each message is
    // pinned separately. What must not drift is that BOTH still reject.
    let default = diagnose(
        Flavor::Default,
        "namespace app;\nmodule M { export mut counter = 0 }\n",
    );
    assert!(
        contains(&default, "plain modules cannot declare module-level `mut`"),
        "{default:?}"
    );
    let ml = diagnose(
        Flavor::Ml,
        "namespace app\n\nmodule M\n    export mut counter = 0\n",
    );
    assert!(contains(&ml, "mutable cells cannot be exported"), "{ml:?}");
}

#[test]
fn plain_module_cannot_own_a_cell_on_either_surface() {
    // Implements [MODULES-STATE-TOPLEVEL]: only a state module owns cells.
    both_reject(
        "plain modules cannot declare module-level `mut`",
        "namespace app;\nmodule M {\n    mut counter = 0\n    export fn get() = counter\n}\n",
        "namespace app\n\nmodule M\n    mut counter = 0\n    export get () = counter\n",
    );
}

#[test]
fn duplicate_module_items_are_rejected_on_both_surfaces() {
    // Implements [MODULES-EXPORTS]: one module, one declaration per name.
    both_reject(
        "duplicate declaration `app::M::a`",
        "namespace app;\nmodule M {\n    export let a = 1\n    export let a = 2\n}\n",
        "namespace app\n\nmodule M\n    export a = 1\n    export a = 2\n",
    );
}

#[test]
fn private_module_item_cannot_be_read_from_outside() {
    // Implements [MODULES-EXPORTS]: unmarked items are private.
    both_reject(
        "declaration `app::M::secret` is private",
        "namespace app;\nmodule M { let secret = 1 }\nprint(\"${M::secret}\")\n",
        "namespace app\n\nmodule M\n    secret = 1\n\nprint \"${M::secret}\"\n",
    );
}

#[test]
fn private_nested_module_cannot_be_imported() {
    // Implements [MODULES-EXPORTS], [MODULES-IMPORT]: no private traversal.
    both_reject(
        "module `app::Outer::Priv` is private",
        "namespace app;\nmodule Outer { module Priv { export let v = 1 } }\nimport app::Outer::Priv\n",
        "namespace app\n\nmodule Outer\n    module Priv\n        export v = 1\n\nimport app::Outer::Priv\n",
    );
}

#[test]
fn unknown_imported_member_is_rejected_on_both_surfaces() {
    // Implements [MODULES-IMPORT]: a member import selects an existing export.
    both_reject(
        "unknown imported member `app::M::nope`",
        "namespace app;\nmodule M { export let a = 1 }\nimport app::M::{nope}\n",
        "namespace app\n\nmodule M\n    export a = 1\n\nimport app::M\n    nope\n",
    );
}

#[test]
fn wildcard_import_is_refused_while_the_manifest_forbids_it() {
    // Implements [MODULES-IMPORT]: `[modules].allow_wildcard_imports` gates `*`.
    both_reject(
        "wildcard imports are disabled by `[modules].allow_wildcard_imports`",
        "namespace app;\nmodule M { export let a = 1 }\nimport app::M::*\n",
        "namespace app\n\nmodule M\n    export a = 1\n\nimport app::M\n    *\n",
    );
}

#[test]
fn manifest_opaque_alias_is_refused_rather_than_leaking_its_representation() {
    // Implements [MODULES-OPAQUE-TYPES]: `export opaque type T = int` would
    // expose `int` to clients, so flattening rejects it instead.
    both_reject(
        "opaque alias `UserId` is unsupported by the flat checker",
        "namespace app;\nmodule M { export opaque type UserId = int }\n",
        "namespace app\n\nmodule M\n    export opaque type UserId = int\n",
    );
}

#[test]
fn signature_item_without_an_implementation_is_rejected() {
    // Implements [MODULES-SIGNATURE]: every signature item needs a body.
    both_reject(
        "signature item `missing` has no implementation",
        concat!(
            "namespace app;\n",
            "signature Api {\n    fn add(a: int) -> int\n    fn missing(a: int) -> int\n}\n",
            "module M : Api { fn add(a) = a }\n",
        ),
        concat!(
            "namespace app\n\n",
            "signature Api\n    add : int -> int\n    missing : int -> int\n\n",
            "module M : Api\n    add a = a\n",
        ),
    );
}

#[test]
fn ml_ascription_refuses_a_redundant_export_marker() {
    // Implements [MODULES-EXPORTS]: ML ascription is exact. Default's
    // `: Api + extra` is the sanctioned escape hatch, so this one is ML-only.
    let messages = diagnose(
        Flavor::Ml,
        "namespace app\n\nsignature Api\n    add : int -> int\n\nmodule M : Api\n    export add a = a\n",
    );
    assert!(
        contains(
            &messages,
            "an ascribed module exports exactly its signature"
        ),
        "{messages:?}"
    );
}

#[test]
fn default_ascription_gates_extra_exports_behind_plus_extra() {
    // Implements [MODULES-EXPORTS]: in Default, `: Signature + extra` is the one
    // way an ascribed module may export beyond its signature. ML has no such
    // form — its ascription is exact — so this surface is Default-only and
    // cannot be asserted through a twin pair.
    const WITHOUT: &str = concat!(
        "namespace app;\n",
        "signature Api {\n    fn add(a: int) -> int\n}\n",
        "module M : Api {\n    fn add(a) = a\n    export fn extraOne() = 1\n}\n",
    );
    let refused = diagnose(Flavor::Default, WITHOUT);
    assert!(
        contains(&refused, "extra export `extraOne` requires `+ extra`"),
        "{refused:?}"
    );
    let permitted = diagnose(
        Flavor::Default,
        &WITHOUT.replace(": Api {", ": Api + extra {"),
    );
    assert!(
        permitted.is_empty(),
        "`+ extra` must permit the same export: {permitted:?}"
    );
}

#[test]
fn state_cell_cannot_be_read_outside_its_owning_handler_arms() {
    // Implements [MODULES-STATE-SOURCE-OF-TRUTH]: the effect is the only route.
    both_reject(
        "state cell `cell` is only accessible inside its owning handler arms",
        concat!(
            "namespace app;\n",
            "state module S {\n",
            "    mut cell = 0\n",
            "    export effect Fx { get : fn() -> int }\n",
            "    export fn install(body) = handle Fx\n        get => cell\n    in body()\n",
            "    export fn peek() = cell\n",
            "}\n",
        ),
        concat!(
            "namespace app\n\n",
            "state S\n",
            "    mut cell = 0\n",
            "    export effect Fx\n        get : Unit => int\n",
            "    export install body =\n        handle Fx\n            get => cell\n        in body ()\n",
            "    export peek () = cell\n",
        ),
    );
}

#[test]
fn a_namespace_cannot_own_two_state_modules() {
    // Implements [MODULES-STATE-MODULE]: at most one state owner per namespace.
    both_reject(
        "namespace `app` may contain at most one state module",
        concat!(
            "namespace app;\n",
            "state module A {\n",
            "    mut x = 0\n",
            "    export effect FxA { get : fn() -> int }\n",
            "    export fn install(body) = handle FxA\n        get => x\n    in body()\n",
            "}\n",
            "state module B {\n",
            "    mut y = 0\n",
            "    export effect FxB { get : fn() -> int }\n",
            "    export fn install(body) = handle FxB\n        get => y\n    in body()\n",
            "}\n",
        ),
        concat!(
            "namespace app\n\n",
            "state A\n",
            "    mut x = 0\n",
            "    export effect FxA\n        get : Unit => int\n",
            "    export install body =\n        handle FxA\n            get => x\n        in body ()\n\n",
            "state B\n",
            "    mut y = 0\n",
            "    export effect FxB\n        get : Unit => int\n",
            "    export install body =\n        handle FxB\n            get => y\n        in body ()\n",
        ),
    );
}

#[test]
fn constant_initializer_cycles_are_rejected_before_codegen() {
    // Implements [MODULES-CYCLES], [MODULES-INIT].
    both_reject(
        "constant initializer cycle involving `app::M::a`",
        "namespace app;\nmodule M {\n    export let a = b\n    export let b = a\n}\n",
        "namespace app\n\nmodule M\n    export a = b\n    export b = a\n",
    );
}

#[test]
fn a_module_constant_cannot_depend_on_a_project_declaration() {
    // Implements [MODULES-INIT]: constant initializers stay compile-time pure.
    both_reject(
        "must be a compile-time constant",
        "namespace app;\nmodule A { export let base = 10 }\nmodule B { export let scaled = (A::base * 2) ?: 0 }\n",
        "namespace app\n\nmodule A\n    export base = 10\n\nmodule B\n    export scaled = (A::base * 2) ?: 0\n",
    );
}
