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

type expr = Lit of int | Add of expr * expr | Mul of expr * expr

let rec build s idx depth =
  if depth = 0 then Lit ((hash_at s idx) mod 1000)
  else
    let op = (hash_at s idx) mod 2 in
    let l = build s (idx * 2 + 1) (depth - 1)
    and r = build s (idx * 2 + 2) (depth - 1) in
    if op = 0 then Add (l, r) else Mul (l, r)

let rec eval = function
  | Lit v -> v
  | Add (l, r) -> ((eval l) + (eval r)) mod big_mod
  | Mul (l, r) -> ((eval l) * (eval r)) mod big_mod

let () =
  let seed = read_seed () in
  let acc = ref 0 in
  for t = 0 to 9 do
    acc := (!acc + eval (build (seed + t * 7) 1 14)) mod big_mod
  done;
  Printf.printf "%d\n" !acc
