(* Park-Miller MINSTD RNG *)
let r1 x = (x * 16807) mod 2147483647
let hash_at s i = r1 (r1 (s + i))

let vocab = [| "the"; "quick"; "brown"; "fox"; "jumps"; "over"; "lazy"; "dog" |]

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
  for i = 0 to 199999 do
    let w = vocab.((hash_at seed i) mod 8) in
    acc :=
      !acc
      + String.length w
      + (if String.contains w 'o' then 7 else 0)
      + (if String.length w > 0 && w.[0] = 't' then 3 else 0)
  done;
  Printf.printf "%d\n" !acc
