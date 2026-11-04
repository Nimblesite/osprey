# Profiling

The two programs the [profiler guide](../../../../docs/specs/tooling/0028-Profiler.md)
and its walkthrough film run.

| Program | What it shows |
| --- | --- |
| `busy.osp` | runs until stopped, spending most of its time in `collatz` and the rest in `fib`, so a CPU profile has a clear hot spot |
| `words.osp` | builds and drops 50,000 strings, then exits, so the ARC leak check has work to count |

Profile `busy.osp` in VS Code: open it and press **Run & Profile CPU (Current
File)** in the Processes view of the Osprey Debug container, or run it
yourself and press the flame on its row. Click the sample counter in the
status bar to stop; the results open on their own and the editor shows the
hot lines.

Check `words.osp` for leaks: build it on the ARC backend and arm the exit
census.

```bash
osprey src/osprey/examples/profiling/words.osp --compile --memory=arc -o words
OSPREY_ARC_DEBUG=1 ./words
# [osp-arc] exit: 0 live objects, 0 KiB (+0 immortal)
```

`OSPREY_ARC_DEBUG=2` also lists every object still alive, by kind.
`src/toolchain/crates/osprey-cli/tests/profiling_examples.rs` holds both claims.
