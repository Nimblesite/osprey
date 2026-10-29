# Proof export fixture recording — 2026-10-29

Task: record a real Lean + lean4export run of `theorem id_imp` and make it the
proof kernel's fixture, replacing the hand-written texts in
`examples/proof/kernel/src/fixtures.ospml`.

**Result of the first pass: STOP at the verification gate. The lean4export rev
that matches the pinned Lean v4.25.0 toolchain writes the NDJSON format 3.1.0,
not the 0.1.2 text format the kernel reader took. Per the task rule — if the
exporter does not write 0.1.2, stop, do not adapt the reader — the fixtures
were not replaced on that pass.** Everything around the recording (project,
build, export run, checker run) was completed and is pinned below. The second
pass, at the end of this report, moved the reader to the recorded format and
replaced the fixtures.

## Environment

- `lean --version`:
  `Lean (version 4.25.0, arm64-apple-darwin24.6.0, commit cdd38ac5115bdeec5f609e9126cce00f51ae88b3, Release)`
- lean4export: <https://github.com/leanprover/lean4export> rev
  `cfefafacc917b2d14c5f9bb092ed337f71317a4a` (tag `v4.25.0`, whose own
  `lean-toolchain` is exactly `leanprover/lean4:v4.25.0`)
- lean4checker: <https://github.com/leanprover/lean4checker> rev
  `aa03950243dc18750c1546602ab054609ea8d57d` (tag `v4.25.0`, likewise)
- Both are `[[require]]` pins in `benchmarks/proof/tools/lakefile.toml`;
  neither package has transitive requires, so the build needs network only
  for the two clones.

## Commands

- Build: `lake build lean4export lean4checker IdImp` in
  `benchmarks/proof/tools` — `Build completed successfully (17 jobs.)`,
  18.9 s cold.
- Exporter invocation: `lake exe lean4export IdImp` — exit 0.
- Checker invocation: `lake env lean4checker IdImp` — exit 0, 5.7 s wall
  (cold); `lake env lean4checker -v IdImp` prints `replaying IdImp`, exit 0,
  1.3 s wall (warm).
- Wrapper: `scripts/proof-export.sh` runs the build and exporter, writes the
  export text to stdout and `lean --version` plus both revs to stderr, then
  verifies the header. Current exit: 1, with the stop message below.

## The recorded export (NDJSON 3.1.0)

`sha256` of the recorded text (24 lines, as emitted to stdout):
`2adb6bc5ff4e2fcaf9d804548fc2eaf0753902effbac3d70e1657d1b5998330d`

First and last lines:

    {"meta":{"exporter":{"name":"lean4export","version":"3.1.0"},"format":{"version":"3.1.0"},"lean":{"githash":"cdd38ac5115bdeec5f609e9126cce00f51ae88b3","version":"4.25.0"}}}
    {"thm":{"all":[1],"levelParams":[],"name":1,"type":4,"value":6}}

This is not the 0.1.2 text format with `#NS`/`#ES`/`#EV`/`#EP`/`#EL`/`#THM`
lines; it is a different format entirely, so no single-line edit of it can
produce a 0.1.2 fixture and the kernel reader cannot read it unadapted.

## Why the stop is real, not a version quirk

History of lean4export's output format, from the two repositories:

- `0.1.2` text: last written by commits pinned to `leanprover/lean4:v4.20.0-rc5`
  (e.g. `66be65dab1fcaabddde91a76e82e63beab180846`, `bbed53bcb2bab607e8937f1d94753b451d3b7dca`).
- `2.0.0` text: commits pinned to `v4.22.0-rc3` through `v4.27.0-rc1`
  (last text commit `9847384d3ea63a0e70567cdd6ae26c419402297c`).
- NDJSON: commit `c84075665a99187990c024676e8fe25e5ddbe920`
  ("Transition to ndjson export format", 2026-01-02) and everything after,
  which includes every `v4.25.0`-toolchain tag (`v4.25.0`, `v4.25.1`,
  `v4.25.2`).

No commit both builds on Lean v4.25.0 and writes the 0.1.2 (or any) text
format. Even the 2.0.0 text differs structurally from 0.1.2 (binder lines
carry a name index), so the pinned-toolchain constraint and the 0.1.2 reader
cannot be satisfied together today.

## What was and was not done

Done: the pinned Lake project, the theorem module, the build, the real export
recorded here, the checker verified green, `scripts/proof-export.sh` with the
hard header gate (it is the mechanism that stops), `.gitignore` for `.lake/`.

Not done, and why:

- `idImp ()` not replaced — the recorded text is NDJSON and the reader is not
  adapted. The fixture string is unchanged; only its doc comment now states
  the provenance honestly.
- `idImpBad ()` not derived — there is no recorded 0.1.2 text to make the one
  documented `#EV` edit against. The existing hand-written bad fixture stands.
- No expectedoutput change, no test change — nothing behavioural moved.

Ways out (need a decision, not done unilaterally): pin Lean to a
text-format-matching rev (v4.20.0-rc5 era breaks the CI toolchain pin), or
teach the reader NDJSON as a new format alongside 0.1.2 (a reader change the
task forbade).

## Second pass: the reader reads what the exporter writes

The 0.1.2 text reader was replaced by a reader of the NDJSON format 3.1.0 the
pinned exporter writes (`examples/proof/kernel/src/export.ospml`). The reader
checks the `meta` line's `format.version` and refuses any other version by
name; it reads `str`/`num` names, `succ`/`max`/`imax`/`param` levels, the
`sort`/`bvar`/`const`/`app`/`lam`/`forallE`/`letE` expression forms, and
`thm`/`def`/`axiom` declarations; every other item kind (`inductive`, `quot`,
`opaque`, `mdata`, `proj`, `natVal`, `strVal`, `fvar`, `mvar`) is refused as
`unsupported: <kind>` or `unknown item: <kind>`, never skipped, so no file is
accepted with a declaration the kernel did not check.

- Fixture: `Fixtures::idImp ()` is the recorded export byte for byte, also
  kept on disk as `examples/proof/kernel/exports/id_imp.ndjson`
  (sha256 `2adb6bc5ff4e2fcaf9d804548fc2eaf0753902effbac3d70e1657d1b5998330d`,
  24 lines).
- Bad fixture: `Fixtures::idImpBad ()` is the same text with one edit on line
  22, the inner lambda's `"body":1` (the bound variable `h`) changed to
  `"body":2` (the bound variable `p` itself), so the term is
  `fun (p : Prop) (h : p) => p`, which has type `∀ (p : Prop), p → Prop` and
  is rejected by the kernel's mismatch (`expectedoutput`, last line). Binder
  names in the recorded export are Lean's hygienic names
  (`x._@.IdImp.4195544489._hygCtx._hyg.10`), which is why they appear in the
  rejection message.
- `scripts/proof-export.sh` gates on the meta line carrying
  `"format":{"version":"3.1.0"}` and is a hard stop on any other version;
  `scripts/proof-export.sh --check` byte-compares a fresh export with the
  recorded file and fails with the diff (exit 0 on this file; exit 1 with
  the differing line after a one-byte edit of the fixture). CI runs it in
  the proof job, and the speed gate's `m1_export/lean.setup` makes the same
  comparison against the case's copy on every run.
- `benchmarks/proof/m1_export` is the [PROOF-SPEED] comparison: the Osprey
  side reads `id_imp.ndjson` (a copy of the recorded export) through
  `Checked::exported`; the Lean side runs the built `lean4checker` directly
  on module `IdImp` (`lean.cmd`, with `LEAN_PATH` read from a file that
  `lean.setup` wrote after `lake build`, so `lake env`'s ~0.3 s start-up is
  not on Lean's clock). Measured on this machine, three runs, minimum wall:
  Osprey 4 ms / 2.3 MiB peak, `lean4checker` 440 ms / 51 MiB peak. Lean's
  figure is a cold process replaying one module; it is not the cost of its
  type-checking loop alone, and the README of the benchmark says so.
