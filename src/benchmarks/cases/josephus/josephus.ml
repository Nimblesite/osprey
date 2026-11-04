(* Josephus problem — survivor index for n people, step k=7, via the modular recurrence. *)
let () =
  let acc = ref 0 in
  for i = 2 to 10000000 do
    acc := (!acc + 7) mod i
  done;
  Printf.printf "%d\n" !acc
