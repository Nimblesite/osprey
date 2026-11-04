mutual
  inductive Tree (α : Type) where
    | leaf (value : α)
    | node (children : Forest α)
  inductive Forest (α : Type) where
    | nil
    | cons (head : Tree α) (tail : Forest α)
end

#print Tree.rec
#print Forest.rec

noncomputable def Tree.nodes : Tree α → Nat :=
  Tree.rec (motive_1 := fun _ => Nat) (motive_2 := fun _ => Nat)
    (fun _ => 1) (fun _ count => count + 1) 0 (fun _ _ head tail => head + tail)

noncomputable def Forest.nodes : Forest α → Nat :=
  Forest.rec (motive_1 := fun _ => Nat) (motive_2 := fun _ => Nat)
    (fun _ => 1) (fun _ count => count + 1) 0 (fun _ _ head tail => head + tail)

theorem Tree.leaf_nodes : Tree.nodes (Tree.leaf true) = 1 := rfl
theorem Tree.node_nodes : Tree.nodes (Tree.node (Forest.cons (Tree.leaf true) Forest.nil)) = 2 := rfl
theorem Forest.nil_nodes : Forest.nodes (Forest.nil (α := Bool)) = 0 := rfl
theorem Forest.cons_nodes : Forest.nodes (Forest.cons (Tree.leaf true) (Forest.cons (Tree.node Forest.nil) Forest.nil)) = 2 := rfl

inductive NestedArray where
  | node (children : Array NestedArray)

#print NestedArray.rec
#print NestedArray.rec_1
#print NestedArray.rec_2

noncomputable def NestedArray.nodes : NestedArray → Nat :=
  NestedArray.rec (motive_1 := fun _ => Nat) (motive_2 := fun _ => Nat) (motive_3 := fun _ => Nat)
    (fun _ count => count + 1) (fun _ count => count) 0
    (fun _ _ head tail => head + tail)

theorem NestedArray.node_empty_nodes : NestedArray.nodes (NestedArray.node #[]) = 1 := rfl

theorem NestedArray.node_one_nodes :
    NestedArray.nodes (NestedArray.node #[NestedArray.node #[]]) = 2 := rfl

mutual
  inductive IndexedTree (α : Type) : Nat → Type where
    | leaf (value : α) : IndexedTree α 0
    | fork (children : IndexedForest α n) : IndexedTree α (n + 1)
  inductive IndexedForest (α : Type) : Nat → Type where
    | nil : IndexedForest α 0
    | cons (head : IndexedTree α n) (tail : IndexedForest α m) : IndexedForest α (n + m)
    | route (branches : α → IndexedBranch α) : IndexedForest α 0
  inductive IndexedBranch (α : Type) where
    | branch (tree : IndexedTree α n) : IndexedBranch α
end

#print IndexedTree.rec

noncomputable def IndexedTree.count (pick : α) : IndexedTree α n → Nat :=
  IndexedTree.rec (motive_1 := fun _ _ => Nat) (motive_2 := fun _ _ => Nat)
    (motive_3 := fun _ => Nat)
    (fun _ => 1) (fun {_} _ count => count + 1) 0
    (fun {_ _} _ _ head tail => head + tail) (fun _ each => each pick)
    (fun {_} _ count => count)

theorem IndexedTree.reflexive_count :
    IndexedTree.count true (IndexedTree.fork (IndexedForest.route
      (fun _ => IndexedBranch.branch (IndexedTree.leaf true)))) = 2 := rfl

theorem IndexedTree.indexed_count :
    IndexedTree.count true (IndexedTree.fork (IndexedForest.cons
      (IndexedTree.leaf true) IndexedForest.nil)) = 2 := rfl
