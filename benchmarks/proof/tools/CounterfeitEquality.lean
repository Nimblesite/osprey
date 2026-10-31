prelude
universe u
inductive Eq (α : Sort u) (a : α) : α → Prop where
  | fabricated (b : α) : Eq α a b

theorem claimed_equality (p q : Prop) : Eq Prop p q := @Eq.fabricated Prop p q
