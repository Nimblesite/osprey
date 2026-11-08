# hunt: an Osprey app

**`scripts/hunt` is an Osprey app.** It is written in Osprey and built only from Osprey modules:

- `@osprey/fs` (`src/osprey/packages/fs`): `Walk::tree` walks the corpus and `Path` handles every path.
- `@osprey/json` (`src/osprey/packages/json`): `Doc` decodes each `osprey hunt --json` report into typed records at the edge.

No Python, shell scripts or other languages are allowed here. New hunt tooling is Osprey code in this app. The KLEE engine regression gates are Rust tests in `src/toolchain/crates/osprey-cli/tests/` (`hunt_engine.rs`, `hunt_gc_roots.rs`).

## What it does

It runs `osprey hunt` (KLEE) over every runnable Osprey program in the repository and writes one HTML report [SYMEX-CORPUS].

1. **Inventory** (`src/inventory.ospml`) walks `src/osprey/tests`, `src/osprey/examples`, `docs/book/examples`, `src/osprey/packages` and `src/toolchain/benchmarks`. The programs are:
   - each manifest directory whose `osprey.toml` sets `entry`, counted once as an app;
   - each standalone `.osp` or `.ospml` file;
   - each `.py` file that carries `# osprey: flavor=python`.

   It skips files inside a project or a manifest's `source_roots`, `failscompilation` programs, programs with an `.expectederror` sibling, and `src/osprey/tests/hunt/`, which holds the hunt's own controls.
2. **Sweep** (`src/sweep.ospml`) runs `osprey hunt <program> --json --quiet --max-time 20`, seven at a time, and prints one line per finished program (`[37/404] <program> clean`). Each JSON report is staged as `.part` and renamed into place when its run ends, so an interrupted sweep keeps every finished report.
3. **Report** (`src/report.ospml`) classifies each run:

   | Status | Meaning |
   |---|---|
   | `clean` | No findings. |
   | `engine_limitation` | Only `unmodelled` findings. |
   | `BUG_CANDIDATE` | Any other finding (`memory`, `abort`, `arith`, ...). |
   | `tool_error` | No report, a report that does not decode or has no exploration, exit 1 with no finding, or any other exit code. |

   Programs listed in `src/osprey/tests/HUNT_UNPORTABLE.txt` are hunted too, with their listed reason shown beside the result.

## Run it

From the repository root, after `make build`, with KLEE installed:

```sh
make -C src/osprey/tests/hunt hunt-corpus
# or directly:
./target/release/osprey scripts/hunt --run
```

Outputs:

- `target/hunt/report.html`: the report, with a count for each status and one row per program.
- `target/hunt/runs/<program-slug>.json`: each run's `osprey hunt --json` report.
- `target/hunt/runs/log/<program-slug>.log`: each run's stderr.

## Tests

`scripts/hunt/tests` is the app's Osprey test suite. It needs no KLEE and runs in CI through `make _test_hunt_mock`:

```sh
./target/release/osprey test scripts/hunt/tests
```

It covers:

- the inventory, over path permutations and over the real tree;
- every run classification, from real report shapes;
- HTML escaping and the report page;
- a real sweep of engine-free failures.
