(* Count primes below a limit by trial division — integer % in a tight loop. *)
let rec has_factor n d =
  if d * d > n then false
  else if n mod d = 0 then true
  else has_factor n (d + 1)

let is_prime n =
  if n < 2 then false
  else not (has_factor n 2)

let () =
  let acc = ref 0 in
  for n = 2 to 199999 do
    if is_prime n then incr acc
  done;
  Printf.printf "%d\n" !acc
