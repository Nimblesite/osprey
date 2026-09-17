//! A handler needs no `in`/`do` in either flavor: it can handle the rest of its
//! block ([EFFECTS-HANDLE-REST]) or be a value ([EFFECTS-HANDLER-VALUE]). Both
//! spellings lower to the canonical AST the `in` form already produced, so the
//! checker, the discharge pass and codegen see nothing new.

use osprey_syntax::{parse_program_with_flavor, Flavor};

fn canonical(source: &str, flavor: Flavor) -> String {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(
        parsed.errors.is_empty(),
        "{flavor} syntax errors: {:?}",
        parsed.errors
    );
    osprey_ast::canonical::without_positions(&parsed.program)
}

const EFFECT_DEFAULT: &str = "effect Log { info: fn(string) -> Unit }\n";
const EFFECT_ML: &str = "effect Log\n    info : string => Unit\n";

#[test]
fn handling_the_rest_of_a_block_is_the_in_form_over_that_rest() {
    let rest = concat!(
        "fn run() = {\n",
        "    handle Log {\n",
        "        info m => print(m)\n",
        "    }\n",
        "    perform Log.info(\"one\")\n",
        "    perform Log.info(\"two\")\n",
        "}\n",
    );
    let named = concat!(
        "fn run() = {\n",
        "    handle Log\n",
        "        info m => print(m)\n",
        "    in {\n",
        "        perform Log.info(\"one\")\n",
        "        perform Log.info(\"two\")\n",
        "    }\n",
        "}\n",
    );
    assert_eq!(
        canonical(&format!("{EFFECT_DEFAULT}{rest}"), Flavor::Default),
        canonical(&format!("{EFFECT_DEFAULT}{named}"), Flavor::Default)
    );
}

#[test]
fn ml_handling_the_rest_of_a_block_is_the_in_form_over_that_rest() {
    // The ML twin of the contract above. Cross-flavor equality is not asserted
    // on this shape: `fn run() = { … }` keeps its block in Default while ML
    // collapses a single-value layout block, a difference that predates this
    // syntax. The flavors are held to identical IR by the corpus twins in
    // `tests/regressions/effects/handler_scoping.test.{osp,ospml}`.
    let rest = concat!(
        "run () =\n",
        "    handle Log\n",
        "        info m => print m\n",
        "    perform Log.info \"one\"\n",
        "    perform Log.info \"two\"\n",
    );
    let named = concat!(
        "run () =\n",
        "    handle Log\n",
        "        info m => print m\n",
        "    in\n",
        "        perform Log.info \"one\"\n",
        "        perform Log.info \"two\"\n",
    );
    assert_eq!(
        canonical(&format!("{EFFECT_ML}{rest}"), Flavor::Ml),
        canonical(&format!("{EFFECT_ML}{named}"), Flavor::Ml)
    );
}

#[test]
fn both_flavors_build_the_same_handler_value() {
    let default = concat!(
        "let quiet = handler Log { info m => print(m) }\n",
        "fn run() = quiet(fn() => perform Log.info(\"one\"))\n",
    );
    let ml = concat!(
        "quiet = handler Log\n",
        "    info m => print m\n",
        "run () = quiet (\\() => perform Log.info \"one\")\n",
    );
    assert_eq!(
        canonical(&format!("{EFFECT_DEFAULT}{default}"), Flavor::Default),
        canonical(&format!("{EFFECT_ML}{ml}"), Flavor::Ml)
    );
}

#[test]
fn a_handler_that_names_no_body_and_handles_nothing_is_rejected() {
    for (flavor, source) in [
        (
            Flavor::Default,
            format!("{EFFECT_DEFAULT}fn run() = {{\n    handle Log {{\n        info m => print(m)\n    }}\n}}\n"),
        ),
        (
            Flavor::Ml,
            format!("{EFFECT_ML}run () =\n    handle Log\n        info m => print m\n"),
        ),
    ] {
        let parsed = parse_program_with_flavor(&source, flavor);
        assert!(
            parsed
                .errors
                .iter()
                .any(|error| error.message.contains("names no body")),
            "{flavor} accepted a handler with nothing to handle: {:?}",
            parsed.errors
        );
    }
}
