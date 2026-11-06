let rec hanoi n acc =
  if n = 0 then acc
  else hanoi (n - 1) (hanoi (n - 1) acc + 1)

let () = Printf.printf "%d\n" (hanoi 25 0)
