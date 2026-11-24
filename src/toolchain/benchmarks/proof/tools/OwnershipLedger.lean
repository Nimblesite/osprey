/- [MEM-OWNERSHIP-STATIC-SAFETY] One allocation as seen by the ledger on one
   control-flow edge. Loans name their ultimate lender, after the transitive
   closure in arc/borrows.rs. An owner slot may belong to a frame, field or
   caller. Slot identities are assumed stable for the lifetime of an edge. -/
namespace OwnershipLedger

abbrev Slot := Nat
abbrev Loan := Slot × Slot  -- borrower, ultimate lender

structure Cell where
  live : Bool
  owners : List Slot
  loans : List Loan
  deriving DecidableEq

def dead : Cell := ⟨false, [], []⟩
def owned (owner : Slot) : Cell := ⟨true, [owner], []⟩

/-- Physical liveness is independent of the compiler's list of owner slots. -/
def Valid (c : Cell) : Prop :=
  if c.live then
    ∃ owner, c.owners = [owner] ∧ ∀ loan ∈ c.loans, loan.2 = owner
  else c.owners = [] ∧ c.loans = []

/-- The lowering actions checked at last uses, calls, returns and region exits.
    A move changes the slot that owns the allocation. A live loan prevents a
    move or free until the corresponding last read has discharged it. -/
inductive Action where
  | alloc (owner : Slot)
  | borrow (borrower lender : Slot)
  | read (slot : Slot)
  | endBorrow (borrower : Slot)
  | move (source destination : Slot)
  | free (owner : Slot)

/-- This partial function is the reference checker. A failed guard rejects the edge. -/
def step (c : Cell) : Action → Option Cell
  | .alloc owner => if c = dead then some (owned owner) else none
  | .borrow borrower lender =>
    if c.live && c.owners == [lender] && borrower != lender &&
        !c.loans.any (fun l => l.1 == borrower) then
      some { c with loans := (borrower, lender) :: c.loans }
    else none
  | .read slot =>
    if c.live && (c.owners.contains slot || c.loans.any (fun l => l.1 == slot))
    then some c else none
  | .endBorrow borrower =>
    some { c with loans := c.loans.filter (fun loan => loan.1 != borrower) }
  | .move source destination =>
    if c = owned source then some (owned destination) else none
  | .free owner =>
    if c = owned owner then some { c with live := false, owners := [] } else none

private theorem valid_owned (o : Slot) : Valid (owned o) := by
  simp [Valid, owned]

theorem step_preserves {c next : Cell} {a : Action}
    (valid : Valid c) (accepted : step c a = some next) : Valid next := by
  cases a with
  | alloc o | move _ o =>
    simp only [step] at accepted
    split at accepted <;> cases accepted
    exact valid_owned o
  | free o =>
    simp only [step] at accepted
    split at accepted <;> cases accepted
    rename_i h
    simp [h, Valid, owned]
  | borrow b l =>
    simp only [step] at accepted
    split at accepted
    · rename_i guard
      simp only [Bool.and_eq_true, beq_iff_eq] at guard
      obtain ⟨⟨⟨alive, owners⟩, _⟩, _⟩ := guard
      cases accepted
      simp only [Valid, alive, if_true] at *
      obtain ⟨o, one, oldLoans⟩ := valid
      have : o = l := by simpa [owners] using one.symm
      subst o
      exact ⟨l, owners, by simpa using oldLoans⟩
    · contradiction
  | read slot =>
    simp only [step] at accepted
    split at accepted <;> cases accepted
    exact valid
  | endBorrow b =>
    cases accepted
    unfold Valid at *
    split at valid
    · rename_i alive
      obtain ⟨o, one, all⟩ := valid
      simp only [alive, if_true]
      exact ⟨o, one, fun loan member => all loan (List.mem_filter.mp member).1⟩
    · rename_i deceased
      simp_all

abbrev Heap := Nat → Cell
def Safe (h : Heap) : Prop := ∀ address, Valid (h address)

/-- The same transition relation covers either incoming edge at a join. -/
inductive Path : Heap → Heap → Prop where
  | here (h) : Path h h
  | edge {start before address action next} (preceding : Path start before)
      (accepted : step (before address) action = some next) :
      Path start (fun atAddr => if atAddr = address then next else before atAddr)

theorem path_preserves {start finish} (path : Path start finish)
    (initial : Safe start) : Safe finish := by
  induction path with
  | here => exact initial
  | @edge before address action next _ accepted ih =>
    intro atAddr
    by_cases same : atAddr = address
    · simpa [same] using step_preserves (ih address) accepted
    · simpa [same] using ih atAddr

theorem from_empty {finish} (path : Path (fun _ => dead) finish) : Safe finish :=
  path_preserves path (fun _ => by simp [Valid, dead])

/-- The model agrees with the last-read pattern in single_owner.test.osp:
    discharge one of two aliases, read the other, then move and drop. -/
private def run (actions : List Action) := actions.foldlM step dead

example : run [.alloc 1, .borrow 2 1, .borrow 3 1, .endBorrow 2, .read 3,
               .endBorrow 3, .move 1 4, .free 4] = some dead := by decide

example : run [.alloc 1, .borrow 2 1, .free 1] = none := by decide
example : run [.alloc 1, .borrow 2 1, .move 1 3] = none := by decide
example : run [.alloc 1, .borrow 2 1, .endBorrow 2, .read 2] = none := by decide
example : run [.alloc 1, .move 1 3, .read 1] = none := by decide
example : run [.alloc 1, .free 1, .free 1] = none := by decide

/- [MEM-OWNERSHIP-STATIC-BORROWS] [MEM-OWNERSHIP-STATIC-PATHS]
   [MEM-OWNERSHIP-STATIC-CLOSURES] Values that hold values. A record field, a
   union field and a closure capture are each a slot of the holder. The
   compiler settles every such slot one of three ways (arc/lends.rs `Held`,
   arc/takes.rs): the slot takes the value (`move`), the slot holds it lent
   (`borrow`, discharged when the holder dies), or a field's value is taken
   back out of the slot (`move` from the slot, which the compiler then clears).
   The theorems below are the obligations those three lowerings carry. -/

/-- The cell a borrow leaves: the same owner, with one more loan. -/
private theorem borrowed {c lent : Cell} {field owner : Slot}
    (accepted : step c (.borrow field owner) = some lent) :
    lent = { c with loans := (field, owner) :: c.loans } := by
  simp only [step] at accepted
  split at accepted <;> cases accepted
  rfl

/-- A lent field pins its lender: while the holder lives, the owner can
    neither free the value nor give it away. A holder that is returned, stored
    or sent to a fiber is still alive when its lender's region ends, so the
    region's free is rejected: a lending value cannot leave its scope. -/
theorem lent_pins_owner {c lent : Cell} {field owner : Slot}
    (accepted : step c (.borrow field owner) = some lent) (destination : Slot) :
    step lent (.free owner) = none ∧ step lent (.move owner destination) = none := by
  have shape := borrowed accepted
  subst shape
  simp [step, owned]

/-- The holder frees nothing through a lent field: a drop walk that released
    the value through that slot is rejected. -/
theorem lent_field_not_freed {c lent : Cell} {field owner : Slot}
    (accepted : step c (.borrow field owner) = some lent) :
    step lent (.free field) = none := by
  have shape := borrowed accepted
  subst shape
  simp [step, owned]

/-- A lent field is read through the holder, the holder dies, and the owner
    then frees the value exactly once. -/
theorem lent_then_released {field owner : Slot} (distinct : field ≠ owner) :
    [Action.alloc owner, .borrow field owner, .read field, .endBorrow field,
      .free owner].foldlM step dead = some dead := by
  simp [List.foldlM, step, owned, dead, distinct]

/-- One value lent to two fields of one holder, or to two holders, is still
    freed once, by its owner, after both loans end. -/
theorem lent_twice_released {left right owner : Slot}
    (l : left ≠ owner) (r : right ≠ owner) (apart : right ≠ left) :
    [Action.alloc owner, .borrow left owner, .borrow right owner, .read left,
      .read right, .endBorrow left, .endBorrow right, .free owner].foldlM step dead
      = some dead := by
  simp [List.foldlM, step, owned, dead, l, r, apart, Ne.symm apart]

/-- Taking a field: the value's one owner changes from the holder's slot to
    the taker. -/
theorem take_moves_owner (field taker : Slot) :
    step (owned field) (.move field taker) = some (owned taker) := by
  simp [step]

/-- After a take the holder must not free through the slot it gave up: that
    second free is rejected, which is why the compiler clears the slot. -/
theorem taken_field_not_freed {field taker : Slot} (distinct : field ≠ taker) :
    step (owned taker) (.free field) = none := by
  simp [step, owned, Ne.symm distinct]

/-- A field cannot be taken while anything still borrows the value. -/
theorem take_needs_no_loan {c lent : Cell} {borrower field : Slot}
    (accepted : step c (.borrow borrower field) = some lent) (taker : Slot) :
    step lent (.move field taker) = none :=
  (lent_pins_owner accepted taker).2

/-- A taken field is freed exactly once, by its taker; the holder's own drop
    adds no action for the cleared slot. -/
theorem taken_freed_once (field taker : Slot) :
    [Action.alloc field, .move field taker, .free taker].foldlM step dead = some dead := by
  simp [List.foldlM, step, owned, dead]

/-- Nothing is accepted on a freed allocation but a fresh allocation and the
    end of a borrow that no longer exists: no read, borrow, move or second free. -/
theorem dead_rejects {a : Action} {next : Cell} (accepted : step dead a = some next) :
    (∃ owner, a = .alloc owner) ∨ (∃ borrower, a = .endBorrow borrower) := by
  cases a with
  | alloc owner => exact Or.inl ⟨owner, rfl⟩
  | endBorrow borrower => exact Or.inr ⟨borrower, rfl⟩
  | borrow _ _ | read _ | move _ _ | free _ => simp [step, dead, owned] at accepted

/-- A step on one allocation leaves every other allocation as it was: a holder
    that moves into another holder keeps the loans it took, so the outer holder
    lends in turn and is pinned by the same theorems. -/
theorem edge_frames {start before : Heap} {address other : Nat} {action : Action}
    {next : Cell} (_ : Path start before)
    (_ : step (before address) action = some next) (apart : other ≠ address) :
    (fun atAddr => if atAddr = address then next else before atAddr) other = before other := by
  simp [apart]

end OwnershipLedger
