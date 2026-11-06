/- Primitive computation controls for [PROOF-KERNEL-THEORY]. -/
import Init
set_option autoImplicit false
namespace QuotientCases

def related (_ _ : Nat) : Prop := True

def zeroOnQuotient : Quot related → Nat :=
  Quot.lift (fun _ => 0) (fun _ _ _ => rfl)

theorem lift_mk (n : Nat) : zeroOnQuotient (Quot.mk related n) = 0 := rfl

theorem ind_mk (n : Nat) :
    @Quot.ind Nat related (fun q => q = q)
      (fun a => Eq.refl (Quot.mk related a)) (Quot.mk related n) =
      Eq.refl (Quot.mk related n) := rfl

theorem sound (a b : Nat) : Quot.mk related a = Quot.mk related b :=
  Quot.sound True.intro

end QuotientCases
