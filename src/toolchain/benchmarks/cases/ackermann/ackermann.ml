(* Ackermann–Péter function — deep, non-tail mutual self-recursion. *)
let add a b = a + b
let sub a b = a - b

let rec ack m n =
  if m = 0 then add n 1
  else if n = 0 then ack (sub m 1) 1
  else ack (sub m 1) (ack m (sub n 1))

let () = Printf.printf "%d\n" (ack 3 10)
