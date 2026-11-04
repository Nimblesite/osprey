(* Naive recursive Fibonacci — exercises raw function-call + recursion overhead. *)
let add a b = a + b
let sub a b = a - b

let rec fib n =
  match n with
  | 0 -> 0
  | 1 -> 1
  | _ -> add (fib (sub n 1)) (fib (sub n 2))

let () = Printf.printf "%d\n" (fib 35)
