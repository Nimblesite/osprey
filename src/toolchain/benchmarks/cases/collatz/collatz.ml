(* Collatz (3n+1) stopping time summed over 1..N — integer division (n/2) in deep recursion. *)
let rec collatz n =
  if n = 1 then 0
  else if n mod 2 = 0 then 1 + collatz (n / 2)
  else 1 + collatz ((3 * n) + 1)

let () =
  let acc = ref 0 in
  for i = 1 to 100000 do
    acc := !acc + collatz i
  done;
  Printf.printf "%d\n" !acc
