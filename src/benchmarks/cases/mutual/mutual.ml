let rec is_even n = if n = 0 then true else is_odd (n - 1)
and is_odd n = if n = 0 then false else is_even (n - 1)

let () =
  let acc = ref 0 in
  for i = 1 to 129999 do
    if is_even (i mod 1000) then acc := !acc + 1
  done;
  Printf.printf "%d\n" !acc
