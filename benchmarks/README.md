# Osprey benchmarks

A cross-language performance harness that measures where Osprey sits relative to
**Rust, C, OCaml, and Haskell** on classic compute benchmarks — both **CPU time**
and **peak memory**.

Every benchmark is implemented identically in all five languages, compiled to a
native binary, checked for correct output, then timed. **All source is in this
folder** under `cases/<name>/` so you can read and compare every line:

```
benchmarks/cases/<name>/
  <name>.osp   <name>.rs   <name>.c   <name>.ml   <name>.hs   expected.txt   bench.json
```

```bash
make bench                       # build everything, run the whole suite
BENCH_FILTER=fib make bench      # only cases whose name contains "fib"
zsh benchmarks/run.sh            # run directly (assumes `make build` already ran)
zsh benchmarks/run.sh primes     # direct, single case
```

> **Heads-up on RAM.** With the optimized build (below) the non-allocating cases
> peak at ~1.4 MB — on par with C. The allocating ones balloon under the
> *default* backend, which never reclaims (see *Findings*): `binarytrees` peaks
> near **905 MB**. The **`Osprey (ARC)`** and **`Osprey (GC)`** columns compile
> the same source with `--memory=arc` / `--memory=gc` and bring that to
> **~4.9 MB** and **~24 MB**. Run the default column on a machine with a few GB
> free, or skip it with `BENCH_FILTER`.

Results are written to `benchmarks/results/` (gitignored):

| file | contents |
|------|----------|
| `results.html` | self-contained HTML report — CPU + memory tables styled with the Osprey website CSS (open in a browser) |
| `results.json` | the same data, structured, for tracking over time |
| `hf/*.json`    | raw [hyperfine](https://github.com/sharkdp/hyperfine) exports per case |

`make bench` also **bakes** the tables into the website
(`website/src/_includes/benchmarks-tables.html`, committed) so the
[`/benchmarks`](../website/src/benchmarks.md) page renders them at site-build
time. Both the standalone report and the website page are generated mechanically
by [`report.py`](report.py) — never hand-edit them.

## The benchmarks (22)

Every case prints a single deterministic **integer** result, so output is
byte-comparable across languages (a broken implementation is caught and excluded
from timing) — but that integer is now often a *checksum over non-integer data*
(strings, maps, lists, algebraic trees), so the suite exercises far more than
`int` arithmetic. Four cases additionally run in **two input modes** — a fixed
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
| `exprtree`  | recursive union + records         | constructor allocation + pattern-match dispatch + modular eval | build+evaluate depth-14 trees ×10 |

`listops` is a second **memory** benchmark alongside `binarytrees`: persistent
`listAppend` allocates a fresh spine on every push, and `[head, ...tail]`
recursion walks it. It peaks at **~14 MB** under the default (non-reclaiming)
backend, **~2.6 MB** under ARC and **~3.5 MB** under the GC. `wordfreq` shows the
same effect on the HAMT (59 MB → 2.0 MB under ARC, 4.5 MB under the GC).

## Methodology

1. **Build once, time the binary.** `osprey … --compile` emits a persistent
   native executable; we time *that*, never `--run` (which bundles compile+link
   into the measurement). Comparison languages use their standard optimizing
   release flags (below).
2. **Correctness oracle.** Each binary is run once and its output compared to the
   case's `expected.txt`. A mismatch or build failure is reported and excluded
   from timing — we never publish a number for a program that computed the wrong
   thing.
3. **CPU.** `hyperfine -N --warmup 3 --min-runs 10` per case across all available
   languages → statistical mean ± stddev.
4. **Memory.** `/usr/bin/time` peak resident set size, max over a few runs
   (`-l` on macOS, `-v` on Linux).
5. **Missing toolchains are skipped,** not fatal.

Compile commands (source of truth: [`run.sh`](run.sh)):

| Lang | Command |
|------|---------|
| Osprey  | `osprey <f>.osp --compile` (emits LLVM IR, compiled by clang at `-O2`; override with `OSPREY_OPT`) |
| Osprey (ARC) | `osprey <f>.osp --memory=arc --compile` (same IR; links the Perceus reference-counting runtime archive — [MEM-BACKENDS]) |
| Osprey (GC) | `osprey <f>.osp --memory=gc --compile` (same IR; links the tracing-GC runtime archive — [MEM-BACKENDS]) |
| Rust    | `rustc -C opt-level=3 -C overflow-checks=off -o <bin> <f>.rs` |
| C       | `cc -O2 -o <bin> <f>.c` |
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
- **A case that uses a non-portable builtin is skipped for wasm, not failed.**
  Osprey's wasm runtime is the portable subset (allocator, strings, lists, maps,
  JSON, effects) — it excludes fibers, HTTP, the terminal, and `input`/`random`
  ([0022](../docs/specs/0022-WebAssemblyTarget.md)). So the data-structure cases
  that read a seed via `input()`/`randomBelow` (`wordfreq`, `textstats`,
  `listops`, `exprtree`) have no `Osprey (wasm)` cell, while the pure-compute
  cases (`fib`, `binarytrees`, …) do.

### Constant vs randomized input

The four data-structure cases (`wordfreq`, `textstats`, `listops`, `exprtree`)
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
BENCH_RANDOM=1 BENCH_FILTER=wordfreq make bench   # prints a randomized demo pass
echo 1 | benchmarks/results/bin/wordfreq__osprey   # or drive one binary directly
```

This is the only place randomness enters the suite; everything charted is the
deterministic constant-seed run.

## Reading the numbers fairly

- **Osprey does checked arithmetic on every `+ - * %`** (each returns
  `Result<int, MathError>`, overflow-checked). The others do not by default — we
  even pass `-C overflow-checks=off` to Rust to match its release profile. Part
  of Osprey's CPU gap is the cost of that safety, a real language semantic.
- **Same algorithm everywhere.** Identical *naive* algorithm and parameters in
  every language — no memoization, closed forms, SIMD, or parallelism. We measure
  the language/compiler/runtime, not who is cleverest. Ranges match Osprey's
  half-open `range(a,b)` = `[a,b)` exactly.
- **Osprey loops via `range |> fold`,** not deep linear recursion, because Osprey
  has no tail-call optimization yet (a 1e6-deep recursion overflows the stack).
  The work is identical; only the iteration mechanism differs.
- **OCaml is built without flambda** (stock `ocamlopt`), so its numbers are
  conservative versus an flambda build.
- **Single machine, wall clock.** Treat ratios as indicative; re-run locally.

## Findings

On the author's machine (Apple Silicon, macOS), geometric mean across the 18
benchmarks (open `results/results.html` for the live, per-case numbers):

```
CPU:    Osprey ≈ 1.0× Rust, 1.1× C, 0.7× OCaml, 0.7× Haskell (geomean).
        At parity with the fastest systems languages; faster than OCaml/Haskell.
Memory: ≈ C on 17 of 18 cases (~1.4 MB). One outlier: binarytrees.
```

**Osprey is the fastest of all five languages outright on several cases**
(varies run-to-run, since Osprey/Rust/C are now within measurement noise of each
other) — e.g. `digitsum`, `pascal`, `tak`, `powmod`, `mutual`, `josephus`,
`primes`, `gcdsum`. On the rest it ties C/Rust.

**How it got here.** The first version of this suite measured Osprey 12–89×
slower and using up to 2244× more memory. The cause was **not** the language: the
compiler was handing its LLVM IR to clang with **no optimization flag (`-O0`)**,
so every per-operation `Result` block stayed a live `malloc`. Compiling the IR at
`-O2` lets LLVM prove those allocations non-escaping and delete them outright
(heap → registers): `fib(35)` went from **0.52 s / 1.37 GB to 0.01 s / 1.4 MB**.
See [plan 0010](../docs/plans/0010-cross-language-benchmark-suite.md).

**The one remaining gap — and how reclamation closes it: `binarytrees`.** Its
tree nodes genuinely *escape* — built, held, then checksummed — so the optimizer
cannot statically free them, and the default allocator
(`compiler/runtime/memory_runtime.c`, a `malloc` passthrough) never reclaims:
~905 MB. Allocation funnels through one swappable `@osp_alloc` hook
([MEM-BACKENDS](../docs/specs/0018-MemoryManagement.md)), so a reclaiming backend
drops in at LINK time with **no language change and byte-identical output**. The
conservative tracing collector (`--memory=gc`) takes peak RSS to ~24 MB; Perceus
reference counting (`--memory=arc`, `compiler/runtime/memory_arc.c`) takes it to
**~4.9 MB** — within 3× of C, and the `Osprey (ARC)` column beats the GC on every
allocating case in the table (on `exprtree` it beats C). Both are complete
without a cycle collector because the value heap is acyclic [MEM-ACYCLIC].

The cost is visible in the time column and is exactly what the remaining Perceus
precision tiers address: ARC is **free** on the non-allocating cases (identical
to the default backend, because dup/drop on rodata and on non-escaping values is
elided at compile time) and 2–4.5× on the allocating ones, where every escaping
value still pays a counted retain/release. Borrow inference and drop
specialization remove those from the paths that provably do not need them.

## Not yet benchmarked (and why)

Blocked on language features Osprey doesn't expose today (left out, not faked):

| Benchmark | Blocked on |
|-----------|-----------|
| mandelbrot, n-body, spectral-norm | no `sqrt`/trig stdlib, no `int`↔`float` conversion, float-formatting differs across languages (no exact integer oracle) |
| n-queens, fannkuch | no mutable arrays |
| quicksort, mergesort | recursive `filter`+concat over a persistent `List` currently miscompiles (codegen) — `listops` covers list build + recursive traversal instead |
| sieve of Eratosthenes, matrix-multiply, n-sieve | no mutable arrays |
| pidigits | no arbitrary-precision integers (i64 only) |

`collatz`, `digitsum`, and `isqrt` were unblocked by adding the `intDiv` builtin
(truncating, divide-by-zero-checked integer division — the `/` operator stays
float-only per spec; see [BUILTIN-INTDIV](../docs/specs/0012-Built-InFunctions.md)).
The four data-structure cases (`wordfreq`, `textstats`, `listops`, `exprtree`)
were unblocked by the persistent `Map`/`List`, string builtins, recursive unions,
and the new cryptographically-secure `random`/`randomBelow` builtins
([BUILTIN-RANDOM](../docs/specs/0012-Built-InFunctions.md)). As Osprey grows a
math stdlib, numeric conversions, and mutable arrays, the remaining classics
above become expressible and should be added here.

## Adding a benchmark

1. `mkdir benchmarks/cases/<name>/`.
2. Write `<name>.osp`, verify: `osprey <name>.osp --run`.
3. Add `expected.txt` (exact stdout) and `bench.json` (`{"name","description"}`).
4. Add `<name>.rs`, `<name>.c`, `<name>.ml`, `<name>.hs` — identical algorithm
   and parameters, each printing only the integer result.
5. `BENCH_FILTER=<name> make bench` and confirm every language reports `ok`.
