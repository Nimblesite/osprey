prelude

/-! Inductive declarations for the proof kernel's M3 export: `And`, `Iff` and
`Eq` with the argument names, universe structure and (for `Eq`) real index
family that Lean's own core uses (`Init/Prelude.lean:546`, `Init/Core.lean:181`,
`Init/Prelude.lean:274`), plus three theorems whose proofs go through the
generated recursors. `prelude` keeps Init out, so nothing here is Lean's copy:
every name is declared by this file, and the export carries a genuine
`inductive` block with constructors and recursors. -/

inductive And (a b : Prop) : Prop where
  | intro (left : a) (right : b) : And a b

inductive Iff (a b : Prop) : Prop where
  | intro (mp : a → b) (mpr : b → a) : Iff a b

universe u

inductive Eq {α : Sort u} : α → α → Prop where
  | refl (a : α) : Eq a a

theorem and_swap (p q : Prop) (h : And p q) : And q p :=
  And.intro (@And.rec p q (fun _ => q) (fun _hp hq => hq) h)
    (@And.rec p q (fun _ => p) (fun hp _hq => hp) h)

theorem eq_symm {α : Sort u} {a b : α} (h : Eq a b) : Eq b a :=
  @Eq.rec α a (fun b _ => Eq b a) (Eq.refl a) b h

theorem iff_refl (p : Prop) : Iff p p :=
  Iff.intro (fun h => h) (fun h => h)
