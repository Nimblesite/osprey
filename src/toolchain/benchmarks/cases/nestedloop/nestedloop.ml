(* Triple-nested counting loop accumulating (i*j*k) mod P — nested iteration. *)
let modp = 1000000007

let rec loop_k i j k acc =
  if k = 0 then acc
  else loop_k i j (k - 1) ((acc + i * j * k) mod modp)

let rec loop_j i j n acc =
  if j = 0 then acc
  else loop_j i (j - 1) n (loop_k i j n acc)

let rec loop_i i n acc =
  if i = 0 then acc
  else loop_i (i - 1) n (loop_j i n n acc)

let () = Printf.printf "%d\n" (loop_i 250 250 0)
