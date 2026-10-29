/- Selected exact declaration from openai/math 7ef5c0cc5ca4b0e072b7a050fca09ab24bc6fa3a.
   Original: lean/OAI/Algebra/Determinantal/AmbientCoefficient.lean.
   Apache-2.0; unrelated imports and declarations omitted. -/
import Init
set_option autoImplicit false
namespace OAI.PermanentBorder.AmbientPolar
/-- H,U,V are respectively none, some false, some true. -/
def groupIndex : Option Bool → Fin 3
  | none => 0
  | some false => 1
  | some true => 2

theorem groupIndex_injective : Function.Injective groupIndex := by
  intro a b h
  cases a with
  | none =>
    cases b with
    | none => rfl
    | some b =>
      cases b <;> have h' := congrArg Fin.val h <;> contradiction
  | some a =>
    cases b with
    | none =>
      cases a <;> have h' := congrArg Fin.val h <;> contradiction
    | some b =>
      cases a <;> cases b <;> first | rfl | have h' := congrArg Fin.val h; contradiction

end OAI.PermanentBorder.AmbientPolar
