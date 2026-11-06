(* Sum of decimal digit-sums over 1..N — integer division (n/10) and modulo in recursion. *)
let rec digsum n = if n < 10 then n else (n mod 10) + digsum (n / 10)

let () =
  let acc = ref 0 in
  for i = 1 to 2000000 do
    acc := !acc + digsum i
  done;
  Printf.printf "%d\n" !acc
