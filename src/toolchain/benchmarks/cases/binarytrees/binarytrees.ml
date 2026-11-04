(* Allocate + traverse many short-lived binary trees — allocation/memory stress.
   Faithful port of binarytrees.osp: build 1200 trees of depth 13, sum checks. *)

type tree = Leaf | Node of tree * tree

let rec make d =
  if d = 0 then Node (Leaf, Leaf)
  else Node (make (d - 1), make (d - 1))

let rec check t =
  match t with
  | Leaf -> 0
  | Node (l, r) -> 1 + check l + check r

let () =
  let acc = ref 0 in
  for _ = 0 to 1199 do
    acc := !acc + check (make 13)
  done;
  Printf.printf "%d\n" !acc
