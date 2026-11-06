open Effect
open Effect.Deep
type _ Effect.t += Ask : int Effect.t

let answering (f : (int, int) continuation -> int) = {
  retc = Fun.id;
  exnc = raise;
  effc = (fun (type a) (e : a Effect.t) ->
    match e with
    | Ask -> Some (fun (k : (a, int) continuation) -> f k)
    | _ -> None)
}
let work () = perform Ask + 1
let replace = answering (fun k -> continue k 41)
let around = answering (fun k -> continue k 41 + 100)
let no_resume = answering (fun _k -> 0)
let dead_resume = answering (fun k -> if false then continue k 0 else 0)
let replacement = match_with work () replace
let resumed = match_with work () around
let returned = match_with work () no_resume
let dead_branch = match_with work () dead_resume
let () = Printf.printf "replace=%d\nresume=%d\nno-resume=%d\ndead-resume=%d\n" replacement resumed returned dead_branch
