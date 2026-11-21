//! [MATH-CODE-VERIFICATION]: every row of the fragment reads and emits what
//! [MATH-CODE-MEANING] says, the fixture regenerates its golden byte for byte,
//! and Lean constructs that only Lean checks generate nothing.
#![expect(
    clippy::panic,
    reason = "a refused fixture is a test failure, not a production panic"
)]

use osprey_math::generate;

/// The module body generated from `source`: the lines after `module Case`.
fn body(source: &str) -> Vec<String> {
    let text =
        generate("Case.lean", source).unwrap_or_else(|refusal| panic!("{source}\n{refusal}"));
    let lines = text
        .lines()
        .skip_while(|line| *line != "module Case")
        .skip(1);
    lines
        .map(|line| line.strip_prefix("    ").unwrap_or(line).to_owned())
        .collect()
}

/// The right-hand side generated for one private definition with this body.
fn term(result: &str, lean: &str) -> String {
    let source = format!("private def f (x y : Float) (b : Bool) : {result} := {lean}\n");
    let lines = body(&source);
    assert_eq!(lines.len(), 1, "{lean}: {lines:?}");
    lines
        .concat()
        .split_once(") = ")
        .filter(|(head, _)| head.starts_with("f ("))
        .map_or_else(|| panic!("{lines:?}"), |(_, right)| right.to_owned())
}

/// Every `Lean term => Osprey term` case, as a definition body of type `result`.
fn pairs(result: &str, cases: &[&str]) {
    for case in cases {
        let (lean, osprey) = case.split_once(" => ").unwrap_or_else(|| panic!("{case}"));
        assert_eq!(term(result, lean), osprey, "{case}");
    }
}

#[test]
fn the_fixture_regenerates_its_golden_byte_for_byte() {
    let source = include_str!("fixtures/Kernels.lean");
    let golden = include_str!("fixtures/Kernels.ospml");
    for _ in 0..2 {
        assert_eq!(generate("Kernels.lean", source).as_deref(), Ok(golden));
    }
}

/// Each case is `Lean term => Osprey term`.
const FLOAT_TERMS_KEEP_LEANS_PRECEDENCE_AND_MEANING: &[&str] = &[
    "2 => 2.0",
    "2.5 => 2.5",
    "0 => 0.0",
    "007.50 => 7.50",
    "000.5 => 0.5",
    "1e3 => 1000.0",
    "1e0 => 1.0",
    "1.5e-3 => 0.0015",
    "2.5E+2 => 250.0",
    "12.5e1 => 125.0",
    "5e-1 => 0.5",
    "125e-3 => 0.125",
    "6.283185307179586 => 6.283185307179586",
    "x + y * x => x + y * x",
    "(x + y) * x => (x + y) * x",
    "x - y - x => x - y - x",
    "x - (y - x) => x - (y - x)",
    "x + (y + x) => x + (y + x)",
    "x * (y * x) => x * (y * x)",
    "x * y + x => x * y + x",
    "((x)) => x",
    "x / 2 => x / 2.0",
    "x / 0.5 => x / 0.5",
    "x / -2 => x / (-2.0)",
    "x / y => floatDiv (x, y)",
    "x / 0 => floatDiv (x, 0.0)",
    "x / 0e5 => floatDiv (x, 0.0)",
    "x / 2 / y => floatDiv (x / 2.0, y)",
    "1 / (x + y) => floatDiv (1.0, x + y)",
    "x / y * 2 => floatDiv (x, y) * 2.0",
    "-x => (-1.0) * x",
    "-2.5 => -2.5",
    "-(-2.5) => 2.5",
    "- -x => (-1.0) * ((-1.0) * x)",
    "-x ^ 2 => (-1.0) * pow (x, 2.0)",
    "(-x) ^ 2 => pow ((-1.0) * x, 2.0)",
    "x - -y => x - (-1.0) * y",
    "x * -y => x * ((-1.0) * y)",
    "-x * y => (-1.0) * x * y",
    "x + -2 => x + (-2.0)",
    "-x + y => (-1.0) * x + y",
    "x ^ y => pow (x, y)",
    "x ^ y ^ x => pow (x, pow (y, x))",
    "x ^ (-y) => pow (x, (-1.0) * y)",
    "2 * x ^ 2 => 2.0 * pow (x, 2.0)",
    "-x ^ y ^ 2 => (-1.0) * pow (x, pow (y, 2.0))",
    "x ^ 2 / y ^ 2 => floatDiv (pow (x, 2.0), pow (y, 2.0))",
    "x ^ 2 * y => pow (x, 2.0) * y",
    "Float.pow x y => pow (x, y)",
    "x.pow y => pow (x, y)",
    "x.pow 0.5 + 1 => pow (x, 0.5) + 1.0",
    "Float.sqrt x => sqrt x",
    "x.sqrt => sqrt x",
    "(x + y).sqrt => sqrt (x + y)",
    "x.sqrt.abs => abs (sqrt x)",
    "(x * y).pow 2 => pow (x * y, 2.0)",
    "Float.atan2 y x => atan2 (y, x)",
    "y.atan2 x => atan2 (y, x)",
    "Float.sin (x + 1) => sin (x + 1.0)",
    "Float.abs (-x) => abs ((-1.0) * x)",
    "Float.abs (-2) => abs (-2.0)",
    "Float.exp (Float.cos x) => exp (cos x)",
    "Float.floor x + Float.ceil y => floor x + ceil y",
    "Float.sqrt 2 * x => sqrt 2.0 * x",
];

#[test]
fn float_terms_keep_leans_precedence_and_meaning() {
    pairs("Float", FLOAT_TERMS_KEEP_LEANS_PRECEDENCE_AND_MEANING);
}

#[test]
fn every_float_function_of_the_meaning_table_is_the_osprey_builtin() {
    let unary = [
        "sqrt", "exp", "log2", "log10", "sin", "cos", "tan", "asin", "acos", "atan", "sinh",
        "cosh", "tanh", "asinh", "acosh", "atanh", "floor", "ceil", "abs",
    ];
    for name in unary {
        assert_eq!(
            term("Float", &format!("Float.{name} x")),
            format!("{name} x")
        );
        assert_eq!(term("Float", &format!("x.{name}")), format!("{name} x"));
        assert_eq!(term("Float", &format!("(x).{name}")), format!("{name} x"));
    }
    assert_eq!(term("Float", "Float.log x"), "ln x");
    assert_eq!(term("Float", "x.log"), "ln x");
}

/// Each case is `Lean term => Osprey term`.
const BOOL_TERMS_KEEP_LEANS_PRECEDENCE_AND_MEANING: &[&str] = &[
    "x < y => x < y",
    "x ≤ y => x <= y",
    "x <= y => x <= y",
    "x > y => x > y",
    "x ≥ y => x >= y",
    "x >= y => x >= y",
    "x == y => x == y",
    "x != y => x != y",
    "b ≠ true => b != true",
    "b ≠ (x < y) => b != (x < y)",
    "b == true => b == true",
    "b != (x < y) => b != (x < y)",
    "(b == b) == b => (b == b) == b",
    "x < y && y < x || x == y => x < y && y < x || x == y",
    "(x < y) == (y > x) => (x < y) == (y > x)",
    "x + 1 < y * 2 => x + 1.0 < y * 2.0",
    "true => true",
    "false || b => false || b",
    "x < y && b => x < y && b",
    "x < y ∧ b => x < y && b",
    "b || x < y => b || x < y",
    "b ∨ b => b || b",
    "!b => !b",
    "¬b => !b",
    "!(x < y) => !(x < y)",
    "!x < y => !(x < y)",
    "!b && b => !b && b",
    "b && (b || b) => b && (b || b)",
    "b || b && b => b || b && b",
    "(b || b) && b => (b || b) && b",
    "b && b && b => b && b && b",
    "b ∧ b ∧ b => b && (b && b)",
    "b || b || b => b || b || b",
    "b ∨ b ∨ b => b || (b || b)",
    "x < y ∧ y < 1 || x == y && !b => x < y && y < 1.0 || x == y && !b",
];

#[test]
fn bool_terms_keep_leans_precedence_and_meaning() {
    pairs("Bool", BOOL_TERMS_KEEP_LEANS_PRECEDENCE_AND_MEANING);
}

#[test]
fn branches_and_bindings_become_matches_and_blocks() {
    let source = "private def f (x : Float) (b : Bool) : Float := if b then x else -1\n";
    assert_eq!(
        body(source),
        ["f (x, b) = match b", "    true => x", "    false => -1.0"]
    );
    let source =
        "private def f (x : Float) : Float := let y := x * 2; let z : Float := y + 1; z * y\n";
    assert_eq!(
        body(source),
        ["f x =", "    y = x * 2.0", "    z = y + 1.0", "    z * y"]
    );
    let source = "private def f (x : Float) : Float :=\n  let y := if x < 0 then\n      let n := -x\n      n\n    else x\n  if y > 1 then 1 else\n    let h := y / 2\n    h\n";
    let lines = [
        "f x =",
        "    y = match x < 0.0",
        "        true =>",
        "            n = (-1.0) * x",
        "            n",
        "        false => x",
        "    match y > 1.0",
        "        true => 1.0",
        "        false =>",
        "            h = y / 2.0",
        "            h",
    ];
    assert_eq!(body(source), lines);
    let source = "private def f (x : Float) : Float :=\n  let y := x\n    + 1\n  y\n";
    assert_eq!(body(source), ["f x =", "    y = x + 1.0", "    y"]);
}

#[test]
fn definitions_carry_their_types_documentation_and_visibility() {
    let source = "/-- Twice\n   `x`, closed *) early. -/\ndef twice (x : Float) : Float := 2 * x\n\n/-- Hidden. -/\nprivate def k : Float := 3\ndef limit : Float := k\ndef near (x y : Float) : Bool := twice x < limit\n/-- -/\ndef same (b : Bool) : Bool := b\n";
    let lines = [
        "(** Twice `x`, closed * ) early. *)",
        "export twice x = 2.0 * x",
        "",
        "(** Hidden. *)",
        "k = 3.0",
        "",
        "(** `limit`, generated from Case.lean. *)",
        "export limit = k",
        "",
        "(** `near`, generated from Case.lean. *)",
        "export near (x, y: float) = twice x < limit",
        "",
        "(** `same`, generated from Case.lean. *)",
        "export same (b: bool) = b",
    ];
    assert_eq!(body(source), lines);
}

#[test]
fn a_namespace_names_the_module_and_qualifies_its_definitions() {
    let header = "// Generated from Case.lean by `osprey math` [MATH-CODE-EMIT]. Do not edit: change the Lean source.\n";
    let doc = "(** Mathematics generated from Case.lean. *)\n";
    let two = "namespace app.Tools\nprivate def k : Float := 1\nprivate def j : Float := app.Tools.k + k\nend app.Tools\n";
    let expected =
        format!("{header}namespace app\n\n{doc}module Tools\n    k = 1.0\n\n    j = k + k\n");
    assert_eq!(generate("Case.lean", two).as_deref(), Ok(expected.as_str()));
    let one = "namespace Tools\nprivate def k : Float := 1\n";
    let expected = format!("{header}{doc}module Tools\n    k = 1.0\n");
    assert_eq!(generate("Case.lean", one).as_deref(), Ok(expected.as_str()));
    let none = "private def k : Float := 1\n";
    let expected = format!("{header}{doc}module Case\n    k = 1.0\n");
    assert_eq!(
        generate("Case.lean", none).as_deref(),
        Ok(expected.as_str())
    );
    assert_eq!(
        generate("Case.lean", "").as_deref(),
        Ok(format!("{header}{doc}module Case\n").as_str())
    );
}

#[test]
fn opened_float_functions_are_written_as_mathlib_writes_them() {
    let source = "open Float (exp log sqrt atan2)\nnamespace Tools\nopen Float (pow)\nprivate def f (b x : Float) : Float := log x / log b + sqrt (exp x) + atan2 x b + pow x 2 + x.exp + Float.exp x\nend Tools\n";
    let text = generate("Case.lean", source).unwrap_or_else(|refusal| panic!("{refusal}"));
    let line = "    f (b, x) = floatDiv (ln x, ln b) + sqrt (exp x) + atan2 (x, b) + pow (x, 2.0) + exp x + exp x\n";
    assert!(text.ends_with(line), "{text}");
}

#[test]
fn a_source_may_define_a_builtins_name_when_it_never_calls_that_builtin() {
    let source = "open Float (exp)\nprivate def sinh (x : Float) : Float := (exp x - exp (-x)) / 2\nprivate def cosh (x : Float) : Float := (exp x + exp (-x)) / 2\nprivate def tanh (x : Float) : Float := sinh x / cosh x\n";
    let lines = [
        "sinh x = (exp x - exp ((-1.0) * x)) / 2.0",
        "",
        "cosh x = (exp x + exp ((-1.0) * x)) / 2.0",
        "",
        "tanh x = floatDiv (sinh x, cosh x)",
    ];
    assert_eq!(body(source), lines);
}

#[test]
fn what_only_lean_checks_generates_nothing() {
    let source = "/-! Module docs with a `def` inside. -/\n-- a comment with def and /- no opening\n/- block /- nested -/ still a comment def -/\n#eval \"a string -- not a comment \\\" def\"\n#check @Float.sqrt\n  -- an indented continuation\n  |>.toString\n#print Float\n/-- A doc before a check is dropped. -/\n#guard 'x' == 'x' ∧ [1, 2].length == 2\nprivate def k : Float := 1 -- trailing\n#eval k\n";
    assert_eq!(body(source), ["k = 1.0"]);
}

/// Each case is `Lean definition => its generated definition line`: the body fixes every
/// type it can, one inline annotation per still-open group fixes the rest, and no
/// `name : type` header repeats what inference derives [MATH-CODE-EMIT-ELEGANT].
const HEADS_ANNOTATE_ONLY_WHAT_INFERENCE_CANNOT_DERIVE: &[&str] = &[
    "def f (x : Float) : Float := x * 2 => export f x = x * 2.0",
    "def f (x : Float) : Float := Float.sqrt x => export f x = sqrt x",
    "def f (x y : Float) : Float := x / y => export f (x, y) = floatDiv (x, y)",
    "def f (x y : Float) : Float := x * y => export f (x: float, y) = x * y",
    "def f (x y : Float) : Float := x * y + 1 => export f (x, y) = x * y + 1.0",
    "def f (x y : Float) : Bool := x < y => export f (x: float, y) = x < y",
    "def f (x y : Float) : Bool := x < y && y > 0 => export f (x, y) = x < y && y > 0.0",
    "def f (x y z : Float) : Float := x * y + z * z => export f (x: float, y, z) = x * y + z * z",
    "def f (x y z : Float) : Bool := x * y < 1 && z == z => export f (x, y, z: float) = x * y < 1.0 && z == z",
    "def f (a b : Bool) : Bool := a && b => export f (a, b) = a && b",
    "def f (a b : Bool) : Bool := a == b => export f (a: bool, b) = a == b",
    "def f (a : Bool) : Bool := !a => export f a = !a",
    "def f (x : Float) : Float := Float.abs x => export f (x: float) = abs x",
    "def f (x : Float) : Float := Float.abs (x + 1) => export f x = abs (x + 1.0)",
    "def f (x y z : Float) : Float := if x < y then z else x => export f (x: float, y, z) = match x < y",
    "def f (x y : Float) : Float := let s := x + y\n  Float.cos s => export f (x, y) =",
    "def f (x y : Float) : Float := let s := x * y\n  s => export f (x: float, y) =",
    "def f (x y : Float) : Float := x => export f (x: float, y: float) = x",
    "def f : Float := 2 => export f = 2.0",
];

#[test]
fn heads_annotate_only_what_inference_cannot_derive() {
    for case in HEADS_ANNOTATE_ONLY_WHAT_INFERENCE_CANNOT_DERIVE {
        let (lean, line) = case.split_once(" => ").unwrap_or_else(|| panic!("{case}"));
        let lines = body(&format!("{lean}\n"));
        assert_eq!(
            lines.get(1).map(String::as_str),
            Some(line),
            "{case}: {lines:?}"
        );
        assert!(
            !lines.iter().any(|text| text.contains(" : ")),
            "{case}: {lines:?}"
        );
    }
}

#[test]
fn a_definition_called_before_pins_its_arguments_to_its_lean_types() {
    let source = "def sq (x y : Float) : Float := x * y\ndef twice (x : Float) : Float := sq x x\ndef same (a b : Bool) : Bool := a == b\ndef both (p q : Bool) : Bool := same p q\ndef g (w q : Float) : Float := w * w + w / q\ndef h (w q : Float) : Float := w / q * g w q\ndef low (w q : Float) : Float := 1 / Float.sqrt ((1 - w * w) * (1 - w * w) + w * w / (q * q))\ndef high (w q : Float) : Float := w * w * low w q\n";
    let heads: Vec<String> = body(source)
        .into_iter()
        .filter(|line| line.starts_with("export"))
        .collect();
    let want = [
        "export sq (x: float, y) = x * y",
        "export twice x = sq (x, x)",
        "export same (a: bool, b) = a == b",
        "export both (p, q) = same (p, q)",
        // Osprey defers `w * w` between open operands, so `w` and `q` stay generic in `g` and `low`, and a
        // caller passing its own open values would get them defaulted to `int`.
        "export g (w, q) = w * w + floatDiv (w, q)",
        "export h (w: float, q) = floatDiv (w, q) * g (w, q)",
        "export low (w, q) = floatDiv (1.0, sqrt ((1.0 - w * w) * (1.0 - w * w) + floatDiv (w * w, q * q)))",
        "export high (w: float, q: float) = w * w * low (w, q)",
    ];
    assert_eq!(heads, want);
}
