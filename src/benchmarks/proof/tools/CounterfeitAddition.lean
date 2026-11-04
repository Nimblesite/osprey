prelude
set_option genCtorIdx false
universe u v w
inductive Eq {α : Sort u} : α → α → Prop where
  | refl (a : α) : Eq a a
inductive Nat where
  | zero
  | succ (n : Nat)
def instAddNat : Nat → Nat → Nat := fun a _ => a
def instHAdd (α : Type u) (add : α → α → α) : α → α → α := add
def HAdd.hAdd (α : Type u) (β : Type v) (γ : Type w)
    (dictionary : α → β → γ) (left : α) (right : β) : γ := dictionary left right

theorem claimed_left (n m : Nat) :
    @Eq Nat (HAdd.hAdd Nat Nat Nat (instHAdd Nat instAddNat) n m) n := Eq.refl n
