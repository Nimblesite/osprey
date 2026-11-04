(* Park-Miller MINSTD RNG *)
let r1 x = (x * 16807) mod 2147483647
let hash_at s i = r1 (r1 (s + i))

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
  let counts = Array.make 8 0 in
  for i = 0 to 199999 do
    let k = (hash_at seed i) mod 8 in
    counts.(k) <- counts.(k) + 1
  done;
  let result = ref 0 in
  for k = 0 to 7 do
    result := !result + (k + 1) * counts.(k)
  done;
  Printf.printf "%d\n" !result
