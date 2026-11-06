/- Syntaxes that reach the same trees are one language with different spelling:
the first isomorphism theorem for sets, applied to parsers. -/
import Init
set_option autoImplicit false
namespace SameSyntax

universe u v w

/-- Two programs are related when they parse to the same tree: `ker p`. -/
def SameTree {L : Type u} {A : Type w} (p : L → A) (x y : L) : Prop := p x = p y

/-- A program's lump: `L / ker p`. -/
abbrev Lumps {L : Type u} {A : Type w} (p : L → A) := Quot (SameTree p)

/-- The tree a whole lump shares. -/
def treeOf {L : Type u} {A : Type w} (p : L → A) : Lumps p → A :=
  Quot.lift p (fun _ _ same => same)

/-- Lumps with the same tree are the same lump. -/
theorem lump_eq_of_tree_eq {L : Type u} {A : Type w} (p : L → A) (q r : Lumps p)
    (same : treeOf p q = treeOf p r) : q = r := by
  induction q using Quot.ind with
  | mk x =>
    induction r using Quot.ind with
    | mk y => exact Quot.sound same

/-- `p₁(L₁) = p₂(L₂) → L₁ / ker p₁ ≅ L₂ / ker p₂`, by a pairing that keeps every tree. -/
theorem same_syntax {L₁ : Type u} {L₂ : Type v} {A : Type w} (p₁ : L₁ → A) (p₂ : L₂ → A)
    (sameImage : ∀ a, (∃ x, p₁ x = a) ↔ (∃ y, p₂ y = a)) :
    ∃ φ : Lumps p₁ → Lumps p₂,
      (∀ q r, φ q = φ r → q = r) ∧
      (∀ s, ∃ q, φ q = s) ∧
      (∀ q, treeOf p₂ (φ q) = treeOf p₁ q) := by
  have reached : ∀ q : Lumps p₁, ∃ y, p₂ y = treeOf p₁ q := by
    intro q
    induction q using Quot.ind with
    | mk x => exact (sameImage (p₁ x)).mp ⟨x, rfl⟩
  let φ : Lumps p₁ → Lumps p₂ := fun q => Quot.mk _ (Classical.choose (reached q))
  have keeps : ∀ q, treeOf p₂ (φ q) = treeOf p₁ q := fun q => Classical.choose_spec (reached q)
  refine ⟨φ, ?_, ?_, keeps⟩
  · intro q r same
    apply lump_eq_of_tree_eq
    rw [← keeps q, ← keeps r, same]
  · intro s
    induction s using Quot.ind with
    | mk y =>
      obtain ⟨x, hx⟩ := (sameImage (p₂ y)).mpr ⟨y, rfl⟩
      exact ⟨Quot.mk _ x, lump_eq_of_tree_eq p₂ _ _ ((keeps _).trans hx)⟩

end SameSyntax
