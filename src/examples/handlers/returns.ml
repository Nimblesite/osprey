open Effect
open Effect.Deep
type _ Effect.t += Ask : int Effect.t

let prefix = "done="
let answering (f : (int, string) continuation -> string) = {
  retc = (fun n -> prefix ^ string_of_int n);
  exnc = raise;
  effc = (fun (type a) (e : a Effect.t) ->
    match e with
    | Ask -> Some (fun (k : (a, string) continuation) -> f k)
    | _ -> None)
}
let work () = perform Ask + 1
let replace = answering (fun k -> continue k 41)
let around = answering (fun k -> continue k 41 ^ "!")
let stop = answering (fun _k -> "stopped")
let () = print_endline (match_with work () replace)
let () = print_endline (match_with work () around)
let () = print_endline (match_with work () stop)
