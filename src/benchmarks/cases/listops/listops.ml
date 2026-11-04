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

let () =
  let seed = read_seed () in
  let acc = ref 0 in
  for t = 0 to 7 do
    let s = seed + t * 131 in
    let xs = Array.init 4000 (fun i -> (hash_at s i) mod 1000) in
    let sum = Array.fold_left ( + ) 0 xs in
    let below = Array.fold_left (fun a x -> if x < 500 then a + 1 else a) 0 xs in
    acc := (!acc + sum + below) mod big_mod
  done;
  Printf.printf "%d\n" !acc
