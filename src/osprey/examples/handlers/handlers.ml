open Effect
open Effect.Deep

type _ Effect.t += Ask : int Effect.t | Name : string Effect.t

let answer = {
  retc = Fun.id;
  exnc = raise;
  effc = (fun (type a) (e : a Effect.t) ->
    match e with
    | Ask -> Some (fun (k : (a, _) continuation) -> continue k 41)
    | _ -> None)
}

let reading (person : string) = {
  retc = Fun.id;
  exnc = raise;
  effc = (fun (type a) (e : a Effect.t) ->
    match e with
    | Name -> Some (fun (k : (a, _) continuation) -> continue k person)
    | _ -> None)
}

let work () = perform Ask + 1
let greet () = "Hello, " ^ perform Name ^ "!"
let ada = reading "Ada"
let grace = reading "Grace"
let first = match_with work () answer
let second = match_with work () answer
let hello_ada = match_with greet () ada
let hello_grace = match_with greet () grace
let hello_again = match_with greet () ada
let () = Printf.printf "%d\n%d\n%s\n%s\n%s\n" first second hello_ada hello_grace hello_again
