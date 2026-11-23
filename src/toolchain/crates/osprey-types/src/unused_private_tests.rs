//! Assertions for [TYPE-WARNINGS-UNUSED-PRIVATE]: a private module function or
//! value no expression of its module names is one warning at its declaration,
//! in every flavor; any read by name, path, field, method or record update
//! keeps it quiet, and exports and ascribed modules are never reported.

use crate::{unused_privates, UNUSED_PRIVATE};
use osprey_syntax::{parse_program_with_flavor, Flavor};

fn warned(flavor: Flavor, source: &str) -> Vec<(String, u32)> {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(parsed.errors.is_empty(), "{source}\n{:?}", parsed.errors);
    unused_privates(&parsed.program)
        .into_iter()
        .map(|warning| {
            assert_eq!(warning.rule, UNUSED_PRIVATE);
            let line = warning.position.map_or(0, |position| position.line);
            (warning.message, line)
        })
        .collect()
}

fn dead(kind: &str, name: &str, line: u32) -> (String, u32) {
    (
        format!("private {kind} `{name}` is never used; delete it"),
        line,
    )
}

#[test]
fn every_unread_private_is_reported_and_every_read_keeps_one_quiet() {
    let source = "namespace shop;\n\
module Cart {\n\
    let limit = 3\n\
    let stale = 9\n\
    fn called(x) = x + limit\n\
    fn orphan(x) = x * 2\n\
    fn piped(x) = x\n\
    fn pathed() = 1\n\
    fn spun(n) = match n { 0 => 0 _ => spun(n - 1) }\n\
    type Item = { price: int }\n\
    fn price(i) = i.price\n\
    module Inner {\n\
        fn hidden() = 5\n\
        export fn shown() = Cart::pathed() + piped(1)\n\
    }\n\
    export fn total(i) = called(price(i)) + Inner::shown()\n\
    export fn unread() = 0\n\
}\n\
module Counter : CounterApi {\n\
    fn step(n) = n + 1\n\
}\n";
    assert_eq!(
        warned(Flavor::Default, source),
        [
            dead("value", "Cart::stale", 4),
            dead("function", "Cart::orphan", 6),
            dead("function", "Cart::Inner::hidden", 13),
        ]
    );
}

#[test]
fn ml_and_python_modules_report_the_same_dead_members() {
    let ml = "module Prices\n    type Spot =\n        x : int\n    rate = 2\n    unusedRate = 3\n    scaled x = x * rate\n    forgotten x = x\n    origin = Spot(x = 0)\n    export moved = origin(x = 1)\n    export apply x = scaled x\n";
    assert_eq!(
        warned(Flavor::Ml, ml),
        [
            dead("value", "Prices::unusedRate", 5),
            dead("function", "Prices::forgotten", 7),
        ]
    );
    let python = "# osprey: flavor=python\n# osprey: namespace=shop\n# osprey: module=Rates\n__all__ = [\"apply\"]\nRATE = 2\nSTALE = 4\ndef scaled(x):\n    return x * RATE\ndef unused(x):\n    return x\ndef apply(x):\n    return scaled(x)\n";
    assert_eq!(
        warned(Flavor::Python, python),
        [
            dead("value", "Rates::STALE", 6),
            dead("function", "Rates::unused", 9)
        ]
    );
}

#[test]
fn top_level_declarations_and_quiet_modules_report_nothing() {
    let source = "fn orphan() = 1\nlet loose = 2\nmodule Quiet {\n    fn helper() = 3\n    export fn api() = helper()\n}\n";
    assert_eq!(warned(Flavor::Default, source), Vec::<(String, u32)>::new());
}
