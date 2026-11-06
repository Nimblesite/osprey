(* Sum of integer square roots over 1..N — Newton's method, integer division heavy. *)
let rec isqrt_iter n x =
  let y = (x + n / x) / 2 in
  if y < x then isqrt_iter n y else x

let isqrt n = if n < 2 then n else isqrt_iter n n

let () =
  let acc = ref 0 in
  for i = 1 to 1000000 do
    acc := !acc + isqrt i
  done;
  Printf.printf "%d\n" !acc
