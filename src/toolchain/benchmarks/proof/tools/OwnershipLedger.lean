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

/- The rules for calls, versions, joins, fibers and region exits
   ([MEM-OWNERSHIP-STATIC-PARAMETERS], -SHARING, -PATHS, -FIBERS), each as the
   property of the transition system that the compiler's refusal enforces. -/

/-- Only its owner's free ends an allocation, and only when nothing borrows
    it: every death is one free of an unborrowed value. -/
theorem only_free_kills {c next : Cell} {a : Action} (alive : c.live = true)
    (accepted : step c a = some next) (ended : next.live = false) :
    ∃ owner, a = .free owner ∧ c = owned owner := by
  cases a with
  | free owner =>
    simp only [step] at accepted
    split at accepted
    · rename_i shape
      exact ⟨owner, rfl, shape⟩
    · contradiction
  | alloc owner | move _ owner =>
    simp only [step] at accepted
    split at accepted <;> cases accepted
    simp [owned] at ended
  | borrow _ _ | read _ =>
    simp only [step] at accepted
    split at accepted <;> cases accepted
    simp [alive] at ended
  | endBorrow _ =>
    cases accepted
    simp [alive] at ended

/-- A value given away is gone from its giver: an argument moved to a callee
    that owns the parameter, a source moved into a derived version, a value
    sent to a channel or returned. A later read, borrow, move or free by the
    giver is rejected. -/
theorem given_is_gone {giver taker : Slot} (distinct : giver ≠ taker) (other : Slot) :
    step (owned taker) (.read giver) = none ∧
    step (owned taker) (.borrow other giver) = none ∧
    step (owned taker) (.move giver other) = none ∧
    step (owned taker) (.free giver) = none := by
  simp [step, owned, distinct, Ne.symm distinct]

/-- A value moves at most once from one slot: a second `await` of a fiber, a
    second `recv` of one message and a second use of a derived version's
    source are each a move from a slot that no longer owns. -/
theorem moved_once {source first second : Slot} (distinct : source ≠ first) :
    [Action.alloc source, .move source first, .move source second].foldlM step dead = none := by
  simp [List.foldlM, step, owned, dead, Ne.symm distinct]

/-- Two paths that meet agree on the owner only when both moved the value or
    neither did: an arm of a `match` that gives a binding away while another
    arm keeps it leaves different owners at the join. -/
theorem join_needs_one_owner {kept moved : Slot}
    (agree : step (owned kept) (.move kept moved) = some (owned kept)) : moved = kept := by
  simp [step, owned] at agree
  exact agree

/-- Borrowing changes neither the owner nor liveness: any number of read-only
    borrowers share one owner. -/
theorem borrow_keeps_owner {c lent : Cell} {borrower owner : Slot}
    (accepted : step c (.borrow borrower owner) = some lent) :
    lent.owners = c.owners ∧ lent.live = c.live := by
  have shape := borrowed accepted
  subst shape
  exact ⟨rfl, rfl⟩

/-- A read is accepted only on a live allocation, through its owner or a
    borrower whose loan stands. -/
theorem read_needs_live {c next : Cell} {slot : Slot}
    (accepted : step c (.read slot) = some next) : c.live = true := by
  simp only [step] at accepted
  split at accepted
  · rename_i guard
    simp only [Bool.and_eq_true] at guard
    exact guard.1
  · contradiction

/- Parts, joins with literals and shared elements
   ([MEM-OWNERSHIP-STATIC-PARAMETERS], -PATHS, -SHARING): a function that
   returns part of a parameter (arc/params.rs `yields_part`), a field taken on
   one path only (arc/phi.rs), and a list filtered out of another
   (arc/versions.rs `take_element_source`). -/

/-- Along every accepted path from the empty heap, every allocation has at
    most one owner: no lowering can make a second one. A list that kept an
    element of a list still alive would be that second owner. -/
theorem one_owner_always {finish : Heap} (path : Path (fun _ => dead) finish)
    (address : Nat) : (finish address).owners.length ≤ 1 := by
  have valid := from_empty path address
  unfold Valid at valid
  split at valid
  · obtain ⟨owner, one, _⟩ := valid
    simp [one]
  · simp [valid.1]

/-- A part taken out of a holder outlives it: the taker owns the part, the
    holder is freed without it, and the heap stays safe. A function that
    returns a field of a parameter it owns does exactly this. -/
theorem part_outlives_holder {heap : Heap} {part holder : Nat} {field taker owner : Slot}
    (apart : part ≠ holder) (held : heap part = owned field)
    (whole : heap holder = owned owner) :
    ∃ after : Heap, Path heap after ∧ after part = owned taker ∧
      (after holder).live = false := by
  have first : step (heap part) (.move field taker) = some (owned taker) := by
    simp [held, step]
  have second : step ((fun atAddr => if atAddr = part then owned taker else heap atAddr) holder)
      (.free owner) = some { owned owner with live := false, owners := [] } := by
    simp [Ne.symm apart, whole, step]
  refine ⟨_, Path.edge (Path.edge (Path.here heap) first) second, ?_, ?_⟩
  · simp [apart]
  · simp

/-- A part taken on one path only is freed once on each path: by its taker
    where it was taken, through the holder's slot where it was not. -/
theorem take_on_one_path (field taker : Slot) :
    [Action.alloc field, .move field taker, .free taker].foldlM step dead = some dead ∧
    [Action.alloc field, .free field].foldlM step dead = some dead := by
  simp [List.foldlM, step, owned, dead]

/-- An arm that yields a literal yields no allocation: the join has nothing to
    free, read or give on that path, so the arm needs no owner. -/
theorem literal_needs_no_owner (slot other : Slot) :
    step dead (.free slot) = none ∧ step dead (.read slot) = none ∧
    step dead (.move slot other) = none := by
  simp [step, dead, owned]

/-- An element a derived list keeps has left its source: the source can no
    longer read it or free it, so a source that is read again is refused. -/
theorem shared_element_refused {source derived : Slot} (distinct : source ≠ derived) :
    [Action.alloc source, .move source derived, .read source].foldlM step dead = none ∧
    [Action.alloc source, .move source derived, .free source].foldlM step dead = none := by
  simp [List.foldlM, step, owned, dead, distinct, Ne.symm distinct]

/-- A capture moved into a closure cell that leaves the function leaves the
    function's frame with it [MEM-OWNERSHIP-STATIC-CLOSURES]: the frame can no
    longer free it, and the cell's own death frees it once. -/
theorem capture_leaves_frame {frame cell : Slot} (distinct : frame ≠ cell) :
    step (owned cell) (.free frame) = none ∧
    [Action.alloc frame, .move frame cell, .free cell].foldlM step dead = some dead := by
  simp [List.foldlM, step, owned, dead, Ne.symm distinct]

/-- A join that makes a value on one path and borrows one on another
    [MEM-OWNERSHIP-STATIC-PATHS]: on the making path the name owns the value
    and frees it once; on the borrowing path the name frees nothing, and the
    lender frees its value once the name's last read has passed. -/
theorem mixed_join_frees_once {name lender : Slot} (distinct : name ≠ lender) :
    [Action.alloc name, .read name, .free name].foldlM step dead = some dead ∧
    [Action.alloc lender, .borrow name lender, .read name, .free name].foldlM step dead = none ∧
    [Action.alloc lender, .borrow name lender, .read name, .endBorrow name,
      .free lender].foldlM step dead = some dead := by
  simp [List.foldlM, step, owned, dead, distinct]

/-- A cell that is rewritten owns each value in turn
    [MEM-OWNERSHIP-STATIC-BORROWS]: the write frees the value it replaces, so
    nothing may still borrow that value or read it afterwards, and the fresh
    value moves in and is freed once through the cell. A function that
    returned what a cell holds would be such a borrower. -/
theorem cell_write_frees_old {cell reader fresh : Slot} (apart : reader ≠ cell) :
    [Action.alloc cell, .borrow reader cell, .free cell].foldlM step dead = none ∧
    [Action.alloc cell, .free cell, .read cell].foldlM step dead = none ∧
    [Action.alloc fresh, .move fresh cell, .free cell].foldlM step dead = some dead := by
  simp [List.foldlM, step, owned, dead, apart]

end OwnershipLedger
