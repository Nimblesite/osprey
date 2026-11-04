(* Sum of modular exponentiation: sum of i^20 mod P for i in 1..N, naive repeated multiply. *)
let p = 1000000007

let rec powmod base e acc =
  match e with
  | 0 -> acc
  | _ -> powmod base (e - 1) (acc * base mod p)

let () =
  let acc = ref 0 in
  for i = 1 to 999999 do
    acc := (!acc + powmod i 20 1) mod p
  done;
  Printf.printf "%d\n" !acc
