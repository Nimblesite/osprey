import Init

/-!
# Port: infinite stacks, finite prefixes and tails (OAI.PrefixFlows.Interspersed)

Ported from `openai/math` commit 7ef5c0cc5ca4b0e072b7a050fca09ab24bc6fa3a
(source: `OAI/Computability/Interspersed/Stacks.lean`). Upstream is licensed
under the Apache License, Version 2.0 (see
../../../examples/proof/openai/LICENSE.openai-math).

Adaptations from upstream, nothing else:
* `import Mathlib` replaced with `import Init` (plain Lean 4.25.0 stdlib).
* `open scoped ContDiff` removed (irrelevant; it only compiled with Mathlib).


Definitions `Stack`, `push`, `prepend`, `tail` and all eight theorems
`push_zero`, `push_succ`, `prepend_nil`, `prepend_cons`, `prepend_append`,
`tail_push`, `push_inj`, `push_constant` carry the exact upstream statements and proofs. The only
statement-level changes are notational, elaborating to identical terms:
Mathlib's `Type*` replaced by explicit `universe` declarations
(`{A : Type u}` — same universe-polymorphic types), and Mathlib's `ℕ`
notation replaced by `Nat`.
-/

namespace OAI

/-! Infinite stacks, finite prefixes and tails. -/

noncomputable section
namespace PrefixFlows
namespace Interspersed
universe u

abbrev Stack (A : Type u) := Nat → A

def push {A : Type u} (a : A) (L : Stack A) : Stack A
  | 0 => a
  | n + 1 => L n

@[simp] theorem push_zero {A : Type u} (a : A) (L : Stack A) : push a L 0 = a := rfl

@[simp] theorem push_succ {A : Type u} (a : A) (L : Stack A) (n : Nat) :
    push a L (n + 1) = L n := rfl

def prepend {B : Type u} : List B → Stack B → Stack B
  | [], L => L
  | a :: w, L => push a (prepend w L)

@[simp] theorem prepend_nil {B : Type u} (L : Stack B) : prepend [] L = L := rfl

@[simp] theorem prepend_cons {B : Type u} (a : B) (w : List B) (L : Stack B) :
    prepend (a :: w) L = push a (prepend w L) := rfl

theorem prepend_append {B : Type u} (v w : List B) (L : Stack B) :
    prepend (v ++ w) L = prepend v (prepend w L) := by
  induction v with
  | nil => rfl
  | cons a v ih => simp [ih]

def tail {B : Type u} (L : Stack B) : Stack B := fun n => L (n + 1)

@[simp] theorem tail_push {B : Type u} (a : B) (L : Stack B) : tail (push a L) = L := rfl

@[simp] theorem push_inj {A : Type u} {a b : A} {L R : Stack A} :
    push a L = push b R ↔ a = b ∧ L = R := by
  constructor
  · intro h
    exact ⟨congrFun h 0, funext (fun n => congrFun h (n + 1))⟩
  · rintro ⟨rfl, rfl⟩
    rfl

@[simp] theorem push_constant {B : Type u} (b : B) :
    push b (fun _ => b) = (fun _ => b) := by
  funext n
  cases n <;> rfl

end Interspersed
end PrefixFlows
end

end OAI
