//! [MODULES-DIAG] in both flavors: state cells stay in their handler arms,
//! one state module per namespace, constant initialisers stay acyclic and pure,
//! and import suggestions name only public candidates.

use osprey_syntax::Flavor;

use super::{both_reject, contains, diagnose};

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
            "    export fn install(body) = {\n",
            "        handle Fx {\n",
            "            get => cell\n",
            "        }\n",
            "        body()\n",
            "    }\n",
            "    export fn peek() = cell\n",
            "}\n",
        ),
        concat!(
            "namespace app\n",
            "\n",
            "state S\n",
            "    mut cell = 0\n",
            "    export effect Fx\n",
            "        get : Unit => int\n",
            "    export install body =\n",
            "        handle Fx\n",
            "            get => cell\n",
            "        body ()\n",
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
            "    export fn install(body) = {\n",
            "        handle FxA {\n",
            "            get => x\n",
            "        }\n",
            "        body()\n",
            "    }\n",
            "}\n",
            "state module B {\n",
            "    mut y = 0\n",
            "    export effect FxB { get : fn() -> int }\n",
            "    export fn install(body) = {\n",
            "        handle FxB {\n",
            "            get => y\n",
            "        }\n",
            "        body()\n",
            "    }\n",
            "}\n",
        ),
        concat!(
            "namespace app\n",
            "\n",
            "state A\n",
            "    mut x = 0\n",
            "    export effect FxA\n",
            "        get : Unit => int\n",
            "    export install body =\n",
            "        handle FxA\n",
            "            get => x\n",
            "        body ()\n",
            "\n",
            "state B\n",
            "    mut y = 0\n",
            "    export effect FxB\n",
            "        get : Unit => int\n",
            "    export install body =\n",
            "        handle FxB\n",
            "            get => y\n",
            "        body ()\n",
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
fn a_module_constant_cannot_call_a_project_function() {
    // Implements [MODULES-INIT]: constant initializers stay compile-time pure.
    both_reject(
        "must be a compile-time constant",
        "namespace app;\nmodule A { export fn base() = 10 }\nmodule B { export let scaled = A::base() * 2 }\n",
        "namespace app\n\nmodule A\n    export base () = 10\n\nmodule B\n    export scaled = A::base () * 2\n",
    );
}

#[test]
fn a_module_constant_can_depend_on_another_constant() {
    // [MODULES-INIT] Resolving a pure constant does not execute runtime setup.
    for (flavor, source) in [
        (Flavor::Default, "namespace app;\nmodule A { export let base = 10 }\nmodule B { export let scaled = A::base * 2 }\n"),
        (Flavor::Ml, "namespace app\nmodule A\n    export base = 10\nmodule B\n    export scaled = A::base * 2\n"),
    ] {
        let errors = diagnose(flavor, source);
        assert!(errors.is_empty(), "{flavor}: {errors:?}");
    }
}

#[test]
fn misspelled_import_targets_offer_ranked_public_candidates() {
    both_report(
        "unknown import target `app::Dats`; candidates: `app::Data`, `app::Dates`, `app::Datum`",
        "namespace app;\nmodule Datum {}\nmodule Dates {}\nmodule Data {}\nimport app::Dats\n",
        "namespace app\nmodule Datum\n    v = 0\nmodule Dates\n    v = 0\nmodule Data\n    v = 0\nimport app::Dats\n",
    );
}

#[test]
fn misspelled_import_members_only_suggest_exports() {
    both_report(
        "unknown imported member `app::Data::rea`; candidates: `app::Data::read`",
        "namespace app;\nmodule Data { let real = 1\n export let read = 2 }\nimport app::Data::{rea}\n",
        "namespace app\nmodule Data\n    real = 1\n    export read = 2\nimport app::Data\n    rea\n",
    );
}

#[test]
fn import_suggestions_never_reveal_private_intermediate_modules() {
    for (flavor, source) in [
        (Flavor::Default, "namespace app;\nmodule Outer { module Secret { export module Data {} } }\nimport app::Outer::Secre\n"),
        (Flavor::Ml, "namespace app\nmodule Outer\n    module Secret\n        export module Data\n            v = 1\nimport app::Outer::Secre\n"),
    ] {
        let messages = diagnose(flavor, source);
        assert!(contains(&messages, "unknown import target `app::Outer::Secre`"), "{messages:?}");
        assert!(!messages.iter().any(|message| message.contains("Secret")), "{messages:?}");
    }
}

fn both_report(expected: &str, default: &str, ml: &str) {
    for (flavor, source) in [(Flavor::Default, default), (Flavor::Ml, ml)] {
        assert_eq!(diagnose(flavor, source), vec![expected], "{flavor}");
    }
}
