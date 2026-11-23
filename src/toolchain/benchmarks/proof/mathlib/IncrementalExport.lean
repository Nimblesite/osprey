/-
Incremental exports of a pinned root, one NDJSON 3.1.0 file per module in dependency
order ([PROOF-COMPLETION-MATHLIB], plan 0041 milestone 5). Each file holds only the
declarations no earlier file declared; a constant from an earlier module is named,
never repeated. Checking the files in order, each against the environment the earlier
ones built, checks the whole closure once.

  lake exe incremental <root module> <out dir> [first module index] [module count]

Writes `<out dir>/<index>.ndjson` and prints `<index> <module> <declarations>` per file.
The writer is lean4export's own (`Export`, the pinned rev), so every file is in the
format the kernel reads; only the name, level and expression tables restart per file.
-/
import Export
open Lean

/-- The public constants module `idx` declares, sorted, as lean4export's CLI selects them. -/
def declaredIn (env : Environment) (idx : Nat) : Array Name :=
  (env.header.moduleData[idx]!.constNames.filter (!·.isInternal)).qsort Name.lt

/-- One module's file: fresh tables, the metadata line, then its new declarations. -/
def exportModule (env : Environment) (dir : System.FilePath) (idx : Nat) : M Nat := do
  let file ← IO.FS.Handle.mk (dir / s!"{idx}.ndjson") .write
  let saved ← IO.setStdout (IO.FS.Stream.ofHandle file)
  modify fun st => { st with
    visitedNames := ({} : Std.HashMap Name Nat).insert .anonymous 0
    visitedLevels := ({} : Std.HashMap Level Nat).insert .zero 0
    visitedExprs := {}
    noMDataExprs := {} }
  dumpMetadata
  let names := declaredIn env idx
  for name in names do
    modify fun st => { st with noMDataExprs := {} }
    dumpConstant name
  let _ ← IO.setStdout saved
  return names.size

def main (args : List String) : IO UInt32 := do
  let root :: dir :: rest := args | IO.eprintln "usage: incremental <root> <dir> [first] [count]"; return 2
  initSearchPath (← findSysroot)
  let env ← importModules #[{ module := root.toName }] {}
  let modules := env.header.moduleNames
  let first := (rest.head?.bind String.toNat?).getD 0
  let count := ((rest.drop 1).head?.bind String.toNat?).getD modules.size
  IO.FS.createDirAll dir
  M.run env do
    initState env []
    -- Earlier modules count as exported: their constants are named, not repeated.
    for idx in [0:first] do
      for name in env.header.moduleData[idx]!.constNames do
        modify fun st => { st with visitedConstants := st.visitedConstants.insert name }
    for idx in [first:min (first + count) modules.size] do
      let declared ← exportModule env dir idx
      IO.println s!"{idx} {modules[idx]!} {declared}"
  return 0
