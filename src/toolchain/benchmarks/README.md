# Osprey benchmarks

A cross-language performance harness that measures where Osprey sits relative to
**Rust, C, C#, Dart, Codon, OCaml, and Haskell** on classic compute benchmarks — both **elapsed time**
and **peak memory**.

Every benchmark uses the same workload and expected result across languages. Programs are compiled, checked for correct output, then timed; collection implementations and iteration mechanisms differ. **All source is in this
folder** under `cases/<name>/` so you can read and compare every line:

```
src/toolchain/benchmarks/cases/<name>/
  <name>.osp   <name>.rs   <name>.c   <name>.codon   <name>.ml   <name>.hs   expected.txt   bench.json
  osprey.toml          # only Osprey cases that share the module below
```

The list and text cases share one seed reader and hash per language in
`shared/`: Rust `include!`s `seed.rs`, Dart imports `seed.dart`, `run.sh`
compiles `Seed.cs` beside each C# case, and Osprey imports the module in
`shared/bench.osp`. Each case still builds as one compiler invocation, and the
timed loops stay in the case files.

The Osprey policy is shared the one way the language allows — a project
([MODULES-MODEL]). `shared/bench.osp` holds the `Arith` policy
(`Bench::guarded`, which prints `unexpected integer overflow` / `unexpected zero
divisor` exactly as every case did inline) and the byte-identical helpers
(`add`, `sub`, `mul`, `modp`, `hashAt`, `addM`, `mulM`, `seedOf`, `buildList`,
`weigh`, `word`), with `r1` and `randSeed` module-private because only
`hashAt`/`seedOf` use them. A case that uses them carries an
`osprey.toml` beside it naming `[".", "../../shared"]` as its source roots, so
`run.sh` compiles the case DIRECTORY instead of the single file; the manifest's
`name` is the case name, so the artifact lands where it always did. A case that
needs its own recovery value (`binarytrees`, `exprtree`) keeps its handler and
needs no manifest.

```bash
make -C src/toolchain/benchmarks bench                       # build everything, run the whole suite
make -C src/toolchain/benchmarks bench-osprey                # ONLY the Osprey columns; other languages untouched
BENCH_FILTER=fib make -C src/toolchain/benchmarks bench      # only cases whose name contains "fib"
zsh src/toolchain/benchmarks/run.sh            # run directly (assumes `make build` already ran)
zsh src/toolchain/benchmarks/run.sh primes     # direct, single case
BENCH_LANGS=codon zsh src/toolchain/benchmarks/run.sh  # only the named columns; the others keep their records
```

`make -C src/toolchain/benchmarks bench-osprey` is the fast loop for a compiler or runtime change: it
compiles and times only `osprey-arc`, `osprey-gc` and `osprey-wasm`,
never Rust/C/C#/Dart/Codon/OCaml/Haskell. It stages into a temp directory and merges
only the `(case, language)` cells it actually measured, so every other
language's tracked record survives byte-for-byte — and if any case fails to
build, mismatches its oracle or fails timing, nothing is published at all. It needs one prior
`make -C src/toolchain/benchmarks bench` for the baseline. Every run, including a full `make -C src/toolchain/benchmarks bench`, stages and merges this way. Missing toolchains, skipped cases and unavailable measurements preserve the last recorded values. Replacements require complete, finite timing measurements; published files are replaced atomically.

All WebAssembly columns run under Wasmtime with an explicit 4 MiB call-stack budget (`-W max-wasm-stack=4194304`). The `ackermann` workload exhausts Wasmtime's default budget; with the shared budget it prints the required `8189`. This changes host capacity, not the workload or correctness oracle.

> **Memory backends.** The **`Osprey (ARC)`** and **`Osprey (GC)`** columns compile the same source
> with `--memory=arc` / `--memory=gc`, and both reclaim unused values. `BENCH_FILTER` selects cases, not backends.
>
> <!-- binarytrees-results:start -->
> Current measured peaks: `--memory=arc` **3.08 MB** and `--memory=gc` **15.5 MB**.
> <!-- binarytrees-results:end -->

Results are written to `src/toolchain/benchmarks/results/`. The measured outputs below are
**tracked**, so a figure quoted anywhere in the repo can be checked against them;
raw hyperfine exports (`hf/`) are gitignored, and binaries are built in a temporary staging directory:

| file | contents |
|------|----------|
| `results.json` | every published figure, the run that measured it, and each run's CPU, OS and toolchain versions |
| `hf/*.json`    | raw [hyperfine](https://github.com/sharkdp/hyperfine) exports per case |

The report is the website's [`/benchmarks`](../website/src/benchmarks.md) page; there is
no second copy. [`report.py`](report.py) generates its tables mechanically into
`src/toolchain/website/src/_includes/benchmarks-tables.html` (committed), including
the table of runs, machines and toolchain versions behind every figure.

## The benchmarks (24)

Every case prints a single deterministic **integer** result, so output is
byte-comparable across languages (a broken implementation is caught and excluded
from timing) — but that integer is now often a *checksum over non-integer data*
(strings, maps, lists, algebraic trees), so the suite exercises far more than
`int` arithmetic. Six cases additionally run in **two input modes** — a fixed
constant seed (what we time and verify) or a cryptographically-secure random
seed — see [Constant vs randomized input](#constant-vs-randomized-input).

**Recursion-bound**

| Case | What it stresses | Workload |
|------|------------------|----------|
| `fib`       | function-call + recursion overhead   | naive recursive `fib(35)` |
| `ackermann` | deep non-tail recursion              | `ack(3, 10)` |
| `tak`       | heavy ternary self-recursion         | `tak(32, 16, 8)` |
| `hanoi`     | exponential double recursion         | Towers of Hanoi move count, n=25 |
| `pascal`    | un-memoised binomial recursion       | `C(27, 13)` via `C(n-1,k-1)+C(n-1,k)` |
| `coins`     | combinatorial tree recursion (SICP)  | ways to make 600 from `[1,5,10,25,50]` |
| `mutual`    | mutual recursion                     | `isEven`/`isOdd` across a range |

**Iteration / number theory**

| Case | What it stresses | Workload |
|------|------------------|----------|
| `primes`     | integer `%` in a hot loop          | count primes below 200000 (trial division) |
| `gcdsum`     | Euclidean recursion, modulo        | sum `gcd(i, 1234567)`, i in 1..1,999,999 |
| `nestedloop` | nested iteration + arithmetic      | triple loop `250³`, accumulate `(i*j*k) mod 1e9+7` |
| `factorial`  | multiplication-heavy fold          | product `1..10,000,000` mod 1e9+7 |
| `powmod`     | naive modular exponentiation       | sum of `i^20 mod 1e9+7`, i in 1..1,000,000 |
| `josephus`   | modular iteration                  | Josephus survivor, n=10,000,000, k=7 |
| `coprime`    | nested iteration + gcd             | count coprime pairs in a 2000×2000 grid |
| `collatz`    | integer `/` in deep recursion      | sum of Collatz (3n+1) stopping times over 1..100,000 |
| `digitsum`   | integer `/` + `%` in recursion     | sum of decimal digit-sums over 1..2,000,000 |
| `isqrt`      | Newton's method, integer `/`       | sum of integer square roots over 1..1,000,000 |

**Allocation / memory**

| Case | What it stresses | Workload |
|------|------------------|----------|
| `binarytrees` | allocation / GC / memory pressure | build & checksum 1200 trees of depth 13 |

`binarytrees` is the primary **memory** benchmark: it churns millions of small
heap nodes, so peak RSS reveals each language's allocation/GC strategy (Rust
`Box`, C `malloc`/`free`, OCaml/Haskell GC, Osprey's runtime).

**Data structures (non-integer)** — these stress the runtime's collection and
algebraic-type machinery, not just `int` arithmetic. Each draws its data from a
seeded token generator and runs in constant *or* randomized mode (below).

| Case | Data type | What it stresses | Workload |
|------|-----------|------------------|----------|
| `wordfreq`  | `String` keys + `Map<string,int>` | HAMT insert/lookup + string hashing | count 200k tokens, position-weighted checksum |
| `textstats` | `String`                          | immutable string builtins (`length`/`contains`/`startsWith`) in a hot loop | score 200k tokens |
| `listops`   | persistent `List<int>`            | bitmapped-vector-trie build + recursive traversal | build+traverse 4k-element lists ×8 |
| `quicksort` | persistent `List<int>`            | list pattern matching, prepend and concatenation in a first-element-pivot quicksort | sort 2k-element lists ×8, rank-weighted checksum |
| `mergesort` | persistent `List<int>`            | list splitting and merging in a top-down merge sort | sort 2k-element lists ×8, rank-weighted checksum |
| `exprtree`  | recursive union + records         | constructor allocation + pattern-match dispatch + modular eval | build+evaluate depth-14 trees ×10 |

`listops`, `quicksort`, `mergesort` and `wordfreq` exercise persistent list and map allocation. Compare the ARC and GC columns for the same source; the current measurements are in `results/results.json`.

The two sorts build every partition and merged run as a new persistent list, so each allocates heavily. The other languages run the same out-of-place algorithm on their own sequence type: arrays or vectors in C, Rust, C# and Dart, linked lists in Codon, OCaml and Haskell. Their checksum weights each sorted element by its rank, so a wrong order changes the answer, and both sorts share one oracle.

## Methodology

1. **Build once, time the binary.** `osprey … --compile` emits a persistent
   native executable; we time *that*, never `--run` (which bundles compile+link
   into the measurement). Comparison languages use their standard optimizing
   release flags (below).
2. **Correctness oracle.** Each binary is run once and its output compared to the
   case's `expected.txt`. A mismatch or build failure is reported and excluded
   from timing — we never publish a number for a program that computed the wrong
   thing.
3. **Elapsed time.** `hyperfine -N --warmup 3 --min-runs 10` per case across all available
   languages → statistical mean ± stddev.
4. **Memory.** `/usr/bin/time` peak resident set size, max over a few runs
   (`-l` on macOS, `-v` on Linux).
5. **Missing toolchains are skipped,** not fatal.

Compile commands (source of truth: [`run.sh`](run.sh)):

| Lang | Command |
|------|---------|
| Osprey (ARC) | `osprey <f>.osp --memory=arc --compile` (emits LLVM IR, compiled by clang at `-O2`, override with `OSPREY_OPT`; compiler-inserted ownership operations and the reference-counting runtime archive — [MEM-BACKENDS]) |
| Osprey (GC) | `osprey <f>.osp --memory=gc --compile` (same source; links the tracing-GC runtime archive — [MEM-BACKENDS]) |
| Rust    | `rustc -C opt-level=3 -C overflow-checks=off -o <bin> <f>.rs` |
| Rust (checked) | `rustc -C opt-level=3 -C overflow-checks=on -o <bin> <f>.rs` — the same source with Rust's integer overflow checks on. Osprey checks every integer operation ([spec 0037](../../../docs/specs/language/0037-ArithmeticEffects.md)), so this column is the like-for-like baseline; `Rust` and `C` run unchecked arithmetic |
| C       | `cc -O2 -o <bin> <f>.c` |
| Codon   | `codon build -release -exe -o <bin> <f>.codon` — the program written as Python-flavor Osprey would write it. `int` is 64-bit and wraps, unchecked, as in `Rust` and `C` |
| OCaml   | `ocamlopt -O3 -unsafe -o <bin> <f>.ml` |
| Haskell | `ghc -O2 -o <bin> <f>.hs` |
| Osprey (wasm) | `osprey <f>.osp --target=wasm32 --compile -o <f>.wasm` → `wasmtime run` |
| Rust (wasm)   | `rustc --target wasm32-wasip1 -C opt-level=3 -o <f>.wasm` → `wasmtime run` |
| C (wasm)      | `clang --target=wasm32-wasip1 --sysroot=<wasi> -O2 -o <f>.wasm` → `wasmtime run` |

### WebAssembly targets

The three languages with a stock wasm backend — **Osprey, Rust, C** — also
cross-compile the *same source* to `wasm32-wasip1` and run it under
[`wasmtime`](https://wasmtime.dev), giving the `Osprey (wasm)`, `Rust (wasm)`,
and `C (wasm)` columns. This measures the identical program's cost on a portable
VM. Notes and limits:

- **CPU is charted, memory is not** (`—`): each run is `wasmtime run <module>`, so
  peak RSS is dominated by the wasmtime host, not the module's linear memory —
  not comparable to the native columns. The CPU figure *includes* VM
  startup, so wasm is expected to trail native; it's an apples-to-apples
  wasm-vs-wasm comparison, not wasm-vs-native.
- **OCaml and Haskell have no wasm column** — no stock `ocamlopt`/`ghc` wasm
  target (their wasm stories need separate experimental toolchains).
- **`C (wasm)` needs a wasi-sdk** that ships the wasm `compiler-rt` builtins;
  stock `clang` + `wasi-libc` often lacks it, so the column auto-hides when a
  trivial probe fails to link (set `OSPREY_WASI_SYSROOT` to point at one).
- **Target capabilities still apply.** The WASM runtime supports file, input, random, collection and value-effect operations. It rejects unavailable capabilities such as fibers, sockets and general FFI before linking ([spec 0022](../../../docs/specs/compiler/0022-WebAssemblyTarget.md)). All 22 current Osprey benchmark cases built and passed their output oracle on WebAssembly in the 2026-10-03 refresh.

### Constant vs randomized input

The six data-structure cases (`wordfreq`, `textstats`, `listops`, `exprtree`, `quicksort`, `mergesort`)
read the **first line of stdin** to choose a seed for their token generator:

- **`0` (or empty / not a tty)** — a *fixed* seed: the run is fully deterministic.
  This is what the harness feeds (so timings are reproducible and the
  `expected.txt` oracle holds). Every language uses the same Park-Miller MINSTD
  generator, so all six produce byte-identical output.
- **`1`** — a *cryptographically-secure random* seed. In Osprey this calls the
  new `randomBelow` builtin (OS CSPRNG); the other languages draw from
  `/dev/urandom` / their stdlib RNG. The workload is identical but the data is
  unpredictable, so output varies run-to-run and is **not** oracle-checked.

The harness always feeds the constant seed via `hyperfine --input` and a stdin
redirect (which also stops an `input()` call from blocking on a tty). To watch a
case run on fresh random data each time, set `BENCH_RANDOM=1`:

```bash
BENCH_RANDOM=1 BENCH_FILTER=wordfreq make -C src/toolchain/benchmarks bench   # prints a randomized demo pass
```

The Osprey cases fall back to seed `1` if `randomBelow` returns an error, so a successful randomized demo does not establish that fresh entropy was obtained. Everything charted is the deterministic constant-seed run.

## Reading the numbers fairly

- **Arithmetic semantics differ.** Osprey's integer `+ - *` and `%` return
  plain `int`; overflow or a zero divisor requires an explicit `Arith` policy.
  Rust is compiled with `-C overflow-checks=off` for its release profile, and
  Codon's `int` wraps unchecked.
- **Same algorithm everywhere.** Identical *naive* algorithm and parameters in
  every language — no memoization, closed forms, SIMD, or parallelism. We measure
  the language/compiler/runtime, not who is cleverest. Ranges match Osprey's
  half-open `range(a,b)` = `[a,b)` exactly.
- **Osprey loops via `range |> fold`,** not deep linear recursion, because Osprey
  has no tail-call optimization yet (a 1e6-deep recursion overflows the stack).
  The work is identical; only the iteration mechanism differs.
- **OCaml is built without flambda** (stock `ocamlopt`), so its numbers are
  conservative versus an flambda build.
- **Partial reruns preserve older baselines.** The current machine and measured columns are identified on [the benchmark page](../website/src/benchmarks.md). Compare languages only when their measurements come from the same machine and configuration.

## Findings

The current compiler uses plain integer values and explicit arithmetic effects. Historical timings from the Result-arithmetic compiler do not describe this implementation. The [benchmark page](../website/src/benchmarks.md) reports fresh Osprey measurements and separates them from retained language baselines.

ARC and GC reclaim heap allocations without changing the Osprey program or its expected output. The generated `binarytrees` figures above show the memory difference; the timing tables show its runtime cost. Persistent-map operations in `wordfreq` are also structurally different from the mutable hash tables used by several other language implementations, so that comparison measures the collection choice as well as the compiler.

These results do not establish general performance parity with Rust or C. Use the per-case measurements and reproduce the relevant workload before drawing a comparison.

## Not yet benchmarked (and why)

These workloads are absent from the suite. Some need an agreed numeric oracle; others need language features that are not exposed today:

| Benchmark | Blocked on |
|-----------|-----------|
| mandelbrot, n-body, spectral-norm | implementations and a common numeric accuracy/output oracle; `sqrt`, trigonometric builtins and numeric conversions already exist |
| n-queens, fannkuch | no mutable arrays |
| sieve of Eratosthenes, matrix-multiply, n-sieve | no mutable arrays |
| pidigits | no arbitrary-precision integers (i64 only) |

`collatz`, `digitsum`, and `isqrt` were unblocked by adding the `intDiv` builtin
(truncating, divide-by-zero-checked integer division — the `/` operator stays
float-only per spec; see [BUILTIN-INTDIV](../../../docs/specs/runtime/0012-Built-InFunctions.md)).
The initial four data-structure cases (`wordfreq`, `textstats`, `listops`, `exprtree`)
were unblocked by the persistent `Map`/`List`, string builtins, recursive unions,
and the new cryptographically-secure `random`/`randomBelow` builtins
([BUILTIN-RANDOM](../../../docs/specs/runtime/0012-Built-InFunctions.md)). Numeric workloads can now use the math builtins; add them only with comparable implementations and an agreed oracle. Array and arbitrary-precision workloads retain the blockers listed above.

## Adding a benchmark

1. `mkdir src/toolchain/benchmarks/cases/<name>/`.
2. Write `<name>.osp`, verify: `osprey <name>.osp --run`.
3. Add `expected.txt` (exact stdout) and `bench.json` (`{"name","description"}`).
4. Add `<name>.rs`, `<name>.c`, `<name>.codon`, `<name>.ml`, `<name>.hs` — identical algorithm
   and parameters, each printing only the integer result.
5. `BENCH_FILTER=<name> make -C src/toolchain/benchmarks bench` and confirm every language reports `ok`.

## Symbolic-execution comparison (planned)

`benchmarks/symex/` will hold the planted-bug suite that measures `osprey hunt` against stock KLEE, Kani and QuickCheck: eight cases in four languages, `run.sh`, `report.py` and tracked `results/`. It does not exist yet; the contract is `[SYMEX-PROOF]` in [the symbolic execution spec](../../../docs/specs/tooling/SymbolicExecution.md) and delivery is [plan 0034](../../../docs/plans/0034-symbolic-execution.md).
