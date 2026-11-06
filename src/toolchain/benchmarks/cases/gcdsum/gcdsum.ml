(* Sum of gcd(i, K) for i in 1..N — Euclidean-algorithm recursion (modulo heavy). *)
let rec gcd a b = if b = 0 then a else gcd b (a mod b)

let () =
  let acc = ref 0 in
  for i = 1 to 1999999 do
    acc := !acc + gcd i 1234567
  done;
  Printf.printf "%d\n" !acc
