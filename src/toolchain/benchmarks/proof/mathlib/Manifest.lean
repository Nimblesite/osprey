/-
The exhaustive manifest of a pinned Mathlib root ([PROOF-COMPLETION-MATHLIB], plan 0041
milestone 5): every module with its imports, then every declaration the root
reaches, private and opaque ones included, with its kind, module, statement and
value hashes, direct dependencies and the axioms its whole closure assumes.

  lake env lean --run Manifest.lean <root module> > manifest.ndjson

One JSON object per line, modules first, then declarations sorted by name, so
two runs over the same pin are byte-identical. The last line is the totals.
-/
import Lean
open Lean

def kindOf : ConstantInfo → String
  | .axiomInfo _ => "axiom"
  | .defnInfo _ => "def"
  | .thmInfo _ => "thm"
  | .opaqueInfo _ => "opaque"
  | .quotInfo _ => "quot"
  | .inductInfo _ => "inductive"
  | .ctorInfo _ => "ctor"
  | .recInfo _ => "rec"

def valueHash (info : ConstantInfo) : UInt64 :=
  match info.value? (allowOpaque := true) with
  | some value => value.hash
  | none => 0

def moduleOf (env : Environment) (name : Name) : String :=
  match env.getModuleIdxFor? name with
  | some idx => toString (env.header.moduleNames[idx.toNat]!)
  | none => ""

/-- Direct dependencies of `info`, sorted, so the line is stable. -/
def depsOf (info : ConstantInfo) : Array Name :=
  info.getUsedConstantsAsSet.toArray.qsort Name.lt

/-- The axioms each declaration's closure assumes, by an explicit-stack
depth-first walk with one memo entry per declaration: Mathlib's closures are
too deep for recursion. -/
def axiomsOfAll (env : Environment) (names : Array Name) : Std.HashMap Name NameSet := Id.run do
  let mut memo : Std.HashMap Name NameSet := {}
  -- `unsafe` and `partial` implementations may refer to each other: a name
  -- already being expanded is not entered again.
  let mut open_ : NameSet := {}
  for root in names do
    let mut stack : Array (Name × Bool) := #[(root, false)]
    while h : stack.size > 0 do
      let (name, expanded) := stack[stack.size - 1]
      stack := stack.pop
      if memo.contains name then continue
      match env.find? name with
      | none => memo := memo.insert name {}
      | some info =>
        let deps := info.getUsedConstantsAsSet.toArray
        if expanded then
          open_ := open_.erase name
          let own : NameSet := if info matches .axiomInfo _ then NameSet.empty.insert name else {}
          let all := deps.foldl (fun acc dep => (memo.getD dep {}).foldl NameSet.insert acc) own
          memo := memo.insert name all
        else if !open_.contains name then
          open_ := open_.insert name
          stack := stack.push (name, true)
          for dep in deps do
            unless memo.contains dep || open_.contains dep do stack := stack.push (dep, false)
  return memo

def nameList (names : Array Name) : Json := Json.arr (names.map (fun n => Json.str (toString n)))

def main (args : List String) : IO UInt32 := do
  let some root := args.head? | IO.eprintln "usage: Manifest <root module>"; return 2
  initSearchPath (← findSysroot)
  let env ← importModules #[{ module := root.toName }] {} (loadExts := true)
  let modules := env.header.moduleNames
  for idx in [0:modules.size] do
    let imports := env.header.moduleData[idx]!.imports.map (fun i => toString i.module)
    IO.println (Json.compress (Json.mkObj [("module", toString modules[idx]!),
      ("imports", Json.arr (imports.map Json.str))]))
  let names := (env.constants.fold (fun acc name _ => acc.push name) #[]).qsort Name.lt
  let axioms := axiomsOfAll env names
  let mut kinds : Std.HashMap String Nat := {}
  for name in names do
    let some info := env.find? name | continue
    let kind := kindOf info
    kinds := kinds.insert kind (kinds.getD kind 0 + 1)
    let assumed := ((axioms.getD name {}).toArray).qsort Name.lt
    IO.println (Json.compress (Json.mkObj [("name", toString name), ("kind", kind),
      ("module", moduleOf env name), ("levels", info.levelParams.length),
      ("type", toString info.type.hash), ("value", toString (valueHash info)),
      ("deps", nameList (depsOf info)), ("axioms", nameList assumed)]))
  let counts := (kinds.toArray.qsort (fun a b => a.1 < b.1)).map (fun (k, n) => (k, Json.num n))
  IO.println (Json.compress (Json.mkObj [("root", root), ("modules", modules.size),
    ("declarations", names.size), ("kinds", Json.mkObj counts.toList)]))
  return 0
