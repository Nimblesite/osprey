//! [MATH-CODE-REFUSALS]: every code fires on a minimal near-miss, at the
//! line and column of the fault, and a refused source generates nothing.
#![expect(
    clippy::expect_used,
    reason = "a near-miss that generates is the test failure"
)]

use osprey_math::generate;

/// Each case is `line:column Code | source`: the refusal the near-miss gives.
fn refuses(cases: &[&str]) {
    for case in cases {
        let (expected, source) = case.split_once(" | ").expect(case);
        let refusal = generate("Case.lean", source).expect_err(case);
        let (line, column, code) = (refusal.line, refusal.column, refusal.code);
        assert_eq!(
            format!("{line}:{column} {code:?}"),
            expected,
            "{case}\n{refusal}"
        );
        assert!(refusal
            .to_string()
            .starts_with(&format!("{line}:{column}: {code:?}: ")));
        assert!(
            refusal.message.len() > 20,
            "{case}: the refusal names its fix"
        );
    }
}

#[test]
fn commands_outside_the_fragment_are_refused_by_name() {
    refuses(&[
        "1:10 UnsupportedCommand | open Float\n",
        "1:5 UnsupportedCommand | open Nat (succ)\n",
        "1:11 UnsupportedCommand | open Float in\ndef f : Float := 1\n",
        "1:12 UnsupportedFunction | open Float (cbrt)\n",
        "1:15 UnexpectedToken | open Float (exp\n",
        "1:15 UnexpectedToken | open Float (exp, log)\n",
        "2:16 DuplicateName | def exp_of : Float := 1\nopen Float (exp exp)\n",
        "2:4 DuplicateName | open Float (exp)\ndef exp (x : Float) : Float := x\n",
        "2:7 ShadowedName | open Float (log)\ndef f (log : Float) : Float := log\n",
        "2:17 UnknownName | open Float (exp)\ndef f : Float := log 1\n",
        "1:0 UnsupportedCommand | import Mathlib\ndef f : Float := 1\n",
        "1:0 UnsupportedCommand | structure P where\n  x : Float\n",
        "1:0 UnsupportedCommand | @[simp] def f : Float := 1\n",
        "1:0 UnsupportedCommand | noncomputable def f : Float := 1\n",
        "1:0 UnsupportedCommand | partial def f : Float := 1\n",
        "2:8 UnsupportedCommand | def f : Float := 1\nprivate theorem t : True := trivial\n",
        "1:0 UnsupportedCommand | #reduce 1\n",
        "1:0 UnsupportedCommand | theorem t : True := trivial\n",
        "1:0 UnsupportedCommand | variable (x : Float)\n",
        "2:0 UnsupportedCommand | def f : Float := 1\ntheorem t : f = 1 := rfl\n",
        "2:0 UnsupportedCommand | #eval 1\nexample : True := trivial\n",
        "3:0 UnsupportedCommand | def f (x : Float) : Float :=\n  x\nlemma t : True := trivial\n",
        "1:0 UnsupportedCommand | x + 1\n",
        "1:0 UnsupportedCommand | (1)\n",
        "1:0 UnexpectedToken | /- never closed\ndef f : Float := 1\n",
        "1:19 UnexpectedToken | def f : Float := 1 /-- doc /- deep -/ open\n",
    ]);
}

#[test]
fn definitions_must_state_their_parameters_and_types() {
    refuses(&[
        "1:11 UnsupportedType | def f (n : Nat) : Float := 1\n",
        "1:8 UnsupportedType | def f : Int := 1\n",
        "1:6 UnsupportedType | def f := 1\n",
        "1:18 UnsupportedType | def f (x : Float) := x\n",
        "1:11 UnsupportedType | def f (x : ) : Float := x\n",
        "1:6 UnexpectedToken | def f x : Float := x\n",
        "1:6 UnexpectedToken | def f {x : Float} : Float := x\n",
        "1:8 UnexpectedToken | def f (x) : Float := x\n",
        "1:17 UnexpectedToken | def f (x : Float → Float) : Float := 1\n",
        "1:13 UnexpectedToken | def f : Float\n",
        "1:4 UnexpectedToken | def : Float := 1\n",
        "1:4 UnsupportedName | def f' : Float := 1\n",
        "1:4 UnsupportedName | def F : Float := 1\n",
        "1:4 UnsupportedName | def ns.f : Float := 1\n",
        "1:7 UnsupportedName | def f (α : Float) : Float := α\n",
        "1:33 UnsupportedName | def f (x : Float) : Float := let y₁ := x; y₁\n",
        "1:7 ReservedName | def f (sqrt : Float) : Float := sqrt\n",
        "1:4 ReservedName | def sinh (x : Float) : Float := Float.sinh x\n",
        "2:4 ReservedName | def f (x : Float) : Float := x.exp\ndef exp (x : Float) : Float := x\n",
        "2:29 ReservedName | def exp (x : Float) : Float := x\ndef f (x : Float) : Float := Float.exp x\n",
        "2:36 ReservedName | def sqrt (x : Float) : Float := x\ndef f (x : Float) : Float := (x + 1).sqrt\n",
        "2:31 ReservedName | def pow (x y : Float) : Float := x\ndef f (x : Float) : Float := x ^ 2\n",
        "2:4 ReservedName | def f (x : Float) : Float := x ^ 2\ndef pow (x y : Float) : Float := x\n",
        "1:4 ReservedName | def handle : Float := 1\n",
        "1:7 ReservedName | def f (type : Float) : Float := type\n",
        "1:21 ReservedName | def f : Float := let floatDiv := 1; floatDiv\n",
        "2:4 DuplicateName | def f : Float := 1\ndef f : Float := 2\n",
        "1:9 ShadowedName | def f (x x : Float) : Float := x\n",
        "2:7 ShadowedName | def k : Float := 1\ndef f (k : Float) : Float := k\n",
        "1:33 ShadowedName | def f (x : Float) : Float := let x := 1; x\n",
        "2:21 ShadowedName | def k : Float := 1\ndef f : Float := let k := 2; k\n",
    ]);
}

#[test]
fn terms_outside_the_fragment_are_refused_where_they_stand() {
    refuses(&[
        "1:29 UnsupportedTerm | def f (x : Float) : Float := fun y => y\n",
        "1:29 UnsupportedTerm | def f (x : Float) : Float := match x with | _ => 1\n",
        "1:17 UnsupportedTerm | def f : Float := by exact 1\n",
        "1:30 UnsupportedTerm | def f (x : Float) : Bool := x = 1\n",
        "1:30 UnsupportedTerm | def f (x : Float) : Bool := x ≠ 1\n",
        "1:41 TypeMismatch | def f (x : Float) (b : Bool) : Bool := b ≠ x\n",
        "1:32 UnsupportedTerm | def f (x : Float) : Float := (x : Float)\n",
        "1:26 UnsupportedTerm | def f : Float := let y := let z := 1; z; y\n",
        "1:32 NestedConditional | def f (b : Bool) : Float := 1 + if b then 1 else 2\n",
        "1:40 NestedConditional | def f (b : Bool) : Float := Float.sqrt (if b then 1 else 2)\n",
        "1:33 NestedConditional | def f (x : Float) : Float := x * let y := 1; y\n",
        "1:29 UnsupportedFunction | def f (x : Float) : Float := Float.round x\n",
        "1:29 UnsupportedFunction | def f (x : Float) : Float := x.cbrt\n",
        "1:29 UnsupportedFunction | def f (x : Float) : Float := Float.exp2 x\n",
        "1:36 UnsupportedFunction | def f (x : Float) : Float := (x + 1).scaleB\n",
        "1:17 UnsupportedLiteral | def f : Float := 0x1F\n",
        "1:17 UnsupportedLiteral | def f : Float := 1_000\n",
        "1:17 UnsupportedLiteral | def f : Float := 2.5e\n",
        "1:17 LiteralOutOfRange | def f : Float := 1e400\n",
        "1:17 LiteralOutOfRange | def f : Float := 1e999\n",
        "1:17 LiteralOutOfRange | def f : Float := 1e-999\n",
        "1:17 UnknownName | def f : Float := g\n",
        "1:17 UnknownName | def f : Float := k\ndef k : Float := 1\n",
        "1:17 UnknownName | def f : Float := y.sqrt\n",
        "3:17 UnknownName | namespace Tools\ndef k : Float := 1\ndef f : Float := Other.k\n",
    ]);
}

#[test]
fn calls_take_their_declared_arguments_on_one_line() {
    refuses(&[
        "1:31 ArityMismatch | def f (x y : Float) : Float := Float.sqrt x y\n",
        "1:29 ArityMismatch | def f (x : Float) : Float := Float.atan2 x\n",
        "1:29 ArityMismatch | def f (x : Float) : Float := x.atan2\n",
        "2:17 ArityMismatch | def g (x y : Float) : Float := x\ndef f : Float := g 1\n",
        "2:17 ArityMismatch | def g (x : Float) : Float := x\ndef f : Float := g + 1\n",
        "2:28 ArityMismatch | def g (x : Float) : Float := x\ndef f : Float := Float.sqrt g\n",
        "2:17 ArityMismatch | def g (x : Float) : Float := x\ndef f : Float := g.sqrt\n",
        "3:4 MultilineApplication | def f (x : Float) : Float :=\n  Float.sqrt\n    x\n",
        "3:2 MultilineApplication | def f (x : Float) : Float :=\n  x\n  x\n",
        "3:6 MultilineApplication | def f (x : Float) : Float :=\n  let y := x\n      x\n  y\n",
        "4:2 MultilineApplication | def f (x : Float) : Float :=\n  let y := Float.sqrt x\n  (y)\n  2\n",
        "1:31 UnexpectedToken | def f (x : Float) : Float := x x\n",
        "1:40 UnexpectedToken | def f (x : Float) : Float := let y := x y\n",
        "1:19 UnexpectedToken | def f : Float := 1 )\n",
        "1:19 UnexpectedToken | def f : Float := (1\n",
        "1:20 UnexpectedToken | def f : Float := 1 +\n",
        "1:16 UnexpectedToken | def f : Float :=\n",
        "1:17 UnexpectedToken | def f : Float := )\n",
        "1:33 UnknownName | def f (x : Float) : Float := x + theorem t : True := trivial\n",
        "1:27 UnexpectedToken | def f : Float := let y := 1",
        "2:0 UnexpectedToken | def f : Float := let y := 1\n{ }\n",
        "1:39 UnexpectedToken | def f (b : Bool) : Float := if b then 1\n",
        "1:33 UnexpectedToken | def f (b : Bool) : Float := if b 1 else 2\n",
        "1:23 UnexpectedToken | def f : Float := let y = 1; y\n",
        "1:36 UnexpectedToken | def f (x : Float) : Float := (x).sin.cos\n",
    ]);
}

#[test]
fn every_operand_argument_branch_and_body_has_its_type() {
    refuses(&[
        "1:42 TypeMismatch | def f (x : Float) (b : Bool) : Float := x + b\n",
        "1:42 TypeMismatch | def f (x : Float) (b : Bool) : Float := b * x\n",
        "1:32 TypeMismatch | def f (x : Float) : Float := if x then 1 else 2\n",
        "1:45 TypeMismatch | def f (b : Bool) : Float := if b then 1 else b\n",
        "1:28 TypeMismatch | def f (x : Float) : Bool := x\n",
        "1:29 TypeMismatch | def f (x : Float) : Float := x < 1\n",
        "1:28 TypeMismatch | def f (x : Float) : Bool := !x\n",
        "1:28 TypeMismatch | def f (b : Bool) : Float := -b\n",
        "1:29 TypeMismatch | def f (b : Bool) : Bool := b < b\n",
        "1:41 TypeMismatch | def f (x : Float) (b : Bool) : Bool := x && b\n",
        "1:41 TypeMismatch | def f (x : Float) (b : Bool) : Bool := x == b\n",
        "1:42 TypeMismatch | def f (x : Float) (b : Bool) : Float := x ^ b\n",
        "1:42 TypeMismatch | def f (x : Float) (b : Bool) : Float := b ^ x\n",
        "1:34 TypeMismatch | def f (x : Float) : Bool := x < 1 < 2\n",
        "1:39 TypeMismatch | def f (b : Bool) : Float := Float.sqrt b\n",
        "1:28 TypeMismatch | def f (b : Bool) : Float := b.sqrt\n",
        "1:31 TypeMismatch | def f (b : Bool) : Float := (b).sqrt\n",
        "2:19 TypeMismatch | def g (b : Bool) : Float := 1\ndef f : Float := g 2\n",
        "1:33 TypeMismatch | def f : Float := let y : Bool := 1; 2\n",
    ]);
}

#[test]
fn one_namespace_block_holds_every_definition() {
    refuses(&[
        "3:0 NamespaceShape | namespace A\nend A\nnamespace B\nend B\n",
        "1:10 NamespaceShape | namespace a.b.C\nend a.b.C\n",
        "1:9 NamespaceShape | namespace\n",
        "2:0 NamespaceShape | def f : Float := 1\nnamespace A\nend A\n",
        "3:0 NamespaceShape | namespace A\nend A\ndef f : Float := 1\n",
        "2:4 NamespaceShape | namespace A\nend B\n",
        "1:4 NamespaceShape | end A\n",
        "1:0 NamespaceShape | namespace tools\ndef f : Float := 1\nend tools\n",
        "1:0 NamespaceShape | namespace App.Tools\ndef f : Float := 1\nend App.Tools\n",
    ]);
    let refusal = generate("case.lean", "def f : Float := 1\n").expect_err("a lowercase file stem");
    assert_eq!(
        refusal.to_string().split(": ").take(2).collect::<Vec<_>>(),
        ["1:0", "NamespaceShape"]
    );
}
