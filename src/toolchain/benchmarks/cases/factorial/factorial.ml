(* Factorial-style product 1*2*...*N taken mod 1000000007 (matches factorial.osp). *)
let modp = 1000000007

let () =
  let acc = ref 1 in
  for i = 1 to 10000000 do
    acc := (!acc * i) mod modp
  done;
  Printf.printf "%d\n" !acc
