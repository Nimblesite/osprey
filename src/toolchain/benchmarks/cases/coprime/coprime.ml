(* Count coprime pairs (i,j), 1<=i,j<=N — nested iteration + Euclidean gcd. *)
let rec gcd a b = if b = 0 then a else gcd b (a mod b)

let () =
  let n = 2000 in
  let acc = ref 0 in
  for i = n downto 1 do
    for j = n downto 1 do
      if gcd i j = 1 then incr acc
    done
  done;
  Printf.printf "%d\n" !acc
