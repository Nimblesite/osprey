(* Binomial coefficient via naive (un-memoised) Pascal recursion C(n,k)=C(n-1,k-1)+C(n-1,k). *)
let rec binom n k =
  if k = 0 then 1
  else if k = n then 1
  else binom (n - 1) (k - 1) + binom (n - 1) k

let () = Printf.printf "%d\n" (binom 27 13)
