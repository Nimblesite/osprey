let coin k =
  match k with
  | 1 -> 1
  | 2 -> 5
  | 3 -> 10
  | 4 -> 25
  | _ -> 50

let rec ways amount kind =
  if amount = 0 then 1
  else if amount < 0 then 0
  else if kind = 0 then 0
  else ways (amount - coin kind) kind + ways amount (kind - 1)

let () = Printf.printf "%d\n" (ways 600 5)
