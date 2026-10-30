-- Minimal genuine Lean nested-inductive candidate for a retained regression.
inductive NestedTree where
  | node (children : List NestedTree)

#print NestedTree
#print NestedTree.rec
#print NestedTree.rec_1

theorem NestedTree.node_nil : NestedTree.node [] = NestedTree.node [] := rfl
