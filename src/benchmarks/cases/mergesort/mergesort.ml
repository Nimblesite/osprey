(* Park-Miller MINSTD RNG *)
let r1 x = (x * 16807) mod 2147483647
let hash_at s i = r1 (r1 (s + i))

let big_mod = 1000000007

let read_seed () =
  let line = try Some (read_line ()) with End_of_file -> None in
  let m =
    match line with
    | Some s -> (match int_of_string_opt (String.trim s) with Some v -> v | None -> 0)
    | None -> 0
  in
  if m = 0 then 1
  else begin
    Random.self_init ();
    (Random.full_int 2147483646) + 1
  end

(* Naive top-down merge sort: deal the list into two halves by alternating
   elements, sort both, merge. *)
let rec evens = function
  | [] -> []
  | head :: tail -> head :: odds tail

and odds = function
  | [] -> []
  | _ :: tail -> evens tail

let rec merge left right =
  match left, right with
  | [], _ -> right
  | _, [] -> left
  | l :: ls, r :: rs -> if r < l then r :: merge left rs else l :: merge ls right

let rec merge_sort = function
  | [] -> []
  | [only] -> [only]
  | xs -> merge (merge_sort (evens xs)) (merge_sort (odds xs))

let rec weigh xs rank =
  match xs with
  | [] -> 0
  | head :: tail -> (head * rank + weigh tail (rank + 1)) mod big_mod

let () =
  let seed = read_seed () in
  let acc = ref 0 in
  for t = 0 to 7 do
    let s = seed + t * 131 in
    let xs = List.init 2000 (fun i -> (hash_at s i) mod 100000) in
    acc := (!acc + weigh (merge_sort xs) 1) mod big_mod
  done;
  Printf.printf "%d\n" !acc
