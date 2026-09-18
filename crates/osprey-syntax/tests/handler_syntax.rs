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
fn a_handler_over_the_rest_of_a_block_is_accepted_in_both_flavors() {
    // `handle E { arms }` governs the statements that follow it in its block.
    // There is no second spelling: an explicit computation is written by
    // applying a handler VALUE. [EFFECTS-HANDLE-REST]
    let default = concat!(
        "fn run() = {\n",
        "    handle Log {\n",
        "        info m => print(m)\n",
        "    }\n",
        "    perform Log.info(\"one\")\n",
        "    perform Log.info(\"two\")\n",
        "}\n",
    );
    let ml = concat!(
        "run () =\n",
        "    handle Log\n",
        "        info m => print m\n",
        "    perform Log.info \"one\"\n",
        "    perform Log.info \"two\"\n",
    );
    let _ = canonical(&format!("{EFFECT_DEFAULT}{default}"), Flavor::Default);
    let _ = canonical(&format!("{EFFECT_ML}{ml}"), Flavor::Ml);
}

#[test]
fn in_and_do_handler_forms_are_rejected_in_both_flavors() {
    // "Handler forms with `in` or `do` are rejected in both flavors; there are
    // no compatibility aliases." [EFFECTS-HANDLE-REST]
    for (flavor, source) in [
        (
            Flavor::Default,
            format!("{EFFECT_DEFAULT}fn run() = handle Log {{\n info m => print(m)\n}} do perform Log.info(\"one\")\n"),
        ),
        (
            Flavor::Default,
            format!("{EFFECT_DEFAULT}fn run() = handle Log\n info m => print(m)\nin perform Log.info(\"one\")\n"),
        ),
        (
            Flavor::Ml,
            format!("{EFFECT_ML}run () =\n    handle Log\n        info m => print m\n    in\n        perform Log.info \"one\"\n"),
        ),
    ] {
        let parsed = parse_program_with_flavor(&source, flavor);
        assert!(
            !parsed.errors.is_empty(),
            "{flavor} still accepts an `in`/`do` handler form: {source}"
        );
    }
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
                .any(|error| error.message.contains("nothing to handle")),
            "{flavor} accepted a handler with nothing to handle: {:?}",
            parsed.errors
        );
    }
}

#[test]
fn ml_resume_takes_one_argument_not_the_rest_of_the_expression() {
    // `resume cap + 1` is `(resume cap) + 1`: ML application binds tighter than
    // any operator, and `resume` is an application like every other. Reading
    // the rest of the line as the resumed value made the ML twin of
    // `resume(cap) + 1` compute `resume(cap + 1)` — the same source, a
    // different program, which [FLAVOR-IR-EQUIV] forbids.
    let ml = concat!(
        "effect Ctl\n",
        "    control pick : Unit => int\n",
        "bounded cap =\n",
        "    handler Ctl\n",
        "        pick => resume cap + 1 ?: 0\n",
    );
    let grouped = ml.replace("resume cap + 1", "(resume cap) + 1");
    assert_eq!(
        canonical(ml, Flavor::Ml),
        canonical(&grouped, Flavor::Ml),
        "`resume cap + 1` must group as `(resume cap) + 1`"
    );
}
