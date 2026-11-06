import Mathlib

namespace OAI

/-! Infinite stacks, finite prefixes and tails. -/

noncomputable section
open scoped ContDiff
namespace PrefixFlows
namespace Interspersed
abbrev Stack (A : Type*) := ℕ → A

def push {A : Type*} (a : A) (L : Stack A) : Stack A
  | 0 => a
  | n + 1 => L n

@[simp] theorem push_zero {A : Type*} (a : A) (L : Stack A) : push a L 0 = a := rfl

@[simp] theorem push_succ {A : Type*} (a : A) (L : Stack A) (n : ℕ) :
    push a L (n + 1) = L n := rfl

@[simp] theorem push_inj {A : Type*} {a b : A} {L R : Stack A} :
    push a L = push b R ↔ a = b ∧ L = R := by
  constructor
  · intro h
    exact ⟨congrFun h 0, funext (fun n => congrFun h (n + 1))⟩
  · rintro ⟨rfl, rfl⟩
    rfl

def prepend {B : Type*} : List B → Stack B → Stack B
  | [], L => L
  | a :: w, L => push a (prepend w L)

@[simp] theorem prepend_nil {B : Type*} (L : Stack B) : prepend [] L = L := rfl

@[simp] theorem prepend_cons {B : Type*} (a : B) (w : List B) (L : Stack B) :
    prepend (a :: w) L = push a (prepend w L) := rfl

theorem prepend_append {B : Type*} (v w : List B) (L : Stack B) :
    prepend (v ++ w) L = prepend v (prepend w L) := by
  induction v with
  | nil => rfl
  | cons a v ih => simp [ih]

def tail {B : Type*} (L : Stack B) : Stack B := fun n => L (n + 1)

@[simp] theorem tail_push {B : Type*} (a : B) (L : Stack B) : tail (push a L) = L := rfl

@[simp] theorem push_constant {B : Type*} (b : B) :
    push b (fun _ => b) = (fun _ => b) := by
  funext n
  cases n <;> rfl
end Interspersed
end PrefixFlows
end

end OAI
