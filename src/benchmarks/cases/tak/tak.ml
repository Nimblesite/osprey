let sub a b = a - b

let rec tak x y z =
  if x > y then
    tak (tak (sub x 1) y z) (tak (sub y 1) z x) (tak (sub z 1) x y)
  else z

let () = Printf.printf "%d\n" (tak 32 16 8)
