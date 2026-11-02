# Where PR CI's 57 minutes go, and how to get it to 20–25

Run [41362564685](https://github.com/Nimblesite/osprey/actions/runs/41362564685), commit `6119a002`: **57 minutes of wall clock and 324 runner-minutes, and it failed**. Timings come from the jobs' API timestamps and their step logs.

## Why the wall clock is 57 minutes

```
 0:00 ─ detect changes
 0:54 ─ Build, Format & Analyse ───────── 10.7m   every Linux and Apple test job waits on all of it
11:42 ─ Tests: iOS, macOS and Workbench ─ 45.6m   killed by its 45-minute timeout
57:24 ─ root CI job
```

- **The gate is the wrong shape.** The test jobs only need the compiler artifact, but they wait for format, clippy, deslop, hawk and the 6-minute VSIX bundle too.
- **The two longest test jobs never finished.** The Apple job hit its 45-minute limit and the Rust job its 40-minute limit, so those timeouts set the run's length. The Coverage thresholds job then failed because the Rust coverage file was never produced.
- **The other failures came early.** Mac GPUI and WinUI failed at 15 and 14 minutes on missing LFS pictures and goldens, fixed by `32636a2a` and `8356f2f7`.

## Every job

| Job | Minutes | Where the time goes |
|---|---:|---|
| GPUI host (ubuntu) | 46.8 | UI suites 22.4, smoke 12.8, self-test 5.7, native suite 1.3 |
| iOS, macOS and Workbench | 45.6 (timed out) | iOS, macOS, mobile and bank 32.0, Workbench 12.9 |
| Rust workspace (coverage) | 40.2 (timed out) | One Python package test ran 18.6+ minutes until the kill |
| GPUI host (windows) | 27.5 | Host tests and smoke 13.7, of which the first GPUI build is 10.6 |
| GPUI host (macos) | 23.8 | Smoke 9.5, headless self-test 4.7, window self-test 3.2 |
| Package source coverage | 19.8 | Workbench collection 13.6 |
| WinUI host | 18.9 | Example suites and DLLs 6.6, real windows 4.9 |
| Proof kernel and Lean gate | 17.8 | Full acceptance |
| Windows core | 14.2 | UI suites 5.0, corpus three times 3.6 |
| VS Code extension | 12.5 | Real VS Code under Xvfb |
| Integration | 12.1 | Android 6.4, web compiler 2.6 |
| Build, Format & Analyse | 10.7 | VSIX bundle 6.0 |
| C runtime | 8.3 | Suites 3.6, thresholds 4.1 |
| Package manager (gc, arc) | 5.5 each | |
| Website, Python ports, corpus (gc, arc), wasm | 1.6–4.4 | |

## The biggest costs

### 1. Signal Flow and the synth on CI Linux: 21 minutes

| Suite | Linux CI | Mac CI | Windows CI | This Mac |
|---|---:|---:|---:|---:|
| `packages/signal_flow/tests` | 906s | 37s | 90s | 21s |
| `examples/synth/test` | 371s | 37s | 75s | |

The other UI suites take about 30 seconds on Linux. Run `41370627487`, with an earlier compiler fix, still took 809s and 382s.

**The cause is the garbage collector, and it's fixed.** A Linux container reproduced it, and a profile put 66–97% of the time in `gc_lookup`, called from the conservative scan.

- **Every scanned word went to the hash table.** That includes zeros, which all hash to one slot and walk whatever cluster sits there. In a captured table that cluster was 2,600 slots long, walked once per zero on the heap.
- **The hash clustered on glibc's allocation pattern.** One captured table had a run of 8,671 slots, and a miss averaged 293 probes where about 7 is expected. How bad it got depended on where ASLR placed the heap.
- **The Mac mostly escaped.** The same suite took 16–21 seconds here throughout.

`compiler/runtime/memory_gc.c` now rejects words outside the tracked heap's bounds before touching the table, and uses the full murmur3 finalizer as its hash. Measured in a Linux arm64 container, against the CI x64 figures above:

| Suite | CI before | Container, new hash only | Container, new hash and bounds |
|---|---:|---:|---:|
| `packages/signal_flow/tests` | 906s | 460s | 33s |
| `examples/synth/test` | 371s | | 31s |

### 2. The same tests on every system

| Suite | Runs in |
|---|---|
| Signal Flow tests | GPUI ubuntu, GPUI macos, Windows core, WinUI |
| uitest, Casting and synth tests | GPUI ubuntu, GPUI macos, Windows core |
| Flock, Painting and Storage tests | GPUI ubuntu, GPUI macos (Storage in WinUI too) |
| Native framework suite | GPUI ubuntu, GPUI macos, GPUI windows |
| GPUI headless smoke and self-test | GPUI ubuntu, GPUI macos |
| GPUI `cargo test` | GPUI ubuntu, GPUI macos, GPUI windows |
| Language corpus | Linux gc and arc, Windows three times, macOS, iOS simulator, Android |

These suites run in the language's test harness with no host or window, so a repeat on another system mostly re-proves the same logic. What other systems uniquely prove is native code: the host, the window, the C ABI and the runtime on that platform.

### 3. Every app compiled twice per GPUI job

`run.sh` recompiles the app on each call, so `_gpui-smoke` and `_gpui-self-test` compile the same apps again, one at a time. On Linux each compile is 30–110 seconds while the run it serves takes about 10.

### 4. No cache for the GPUI host

`hosts/gpui/target` is not cached. The first build costs about 5.9 minutes on Linux, 4.5 on Mac and 10.6 on Windows, on every run.

### 5. The Apple job does five things in series

| Part | Minutes |
|---|---:|
| Compiler build, uncached | 3.4 |
| iOS simulator corpus (230 programs) | 7.8 |
| iOS shared and macOS hosts, macOS corpus | 6.0 |
| Issue Inbox and Talon Bank builds | 5.7 |
| Talon Bank UI tests in the simulator | 8.0, of which one test is 4.2 |
| Workbench: Osprey compile | 10.3 |
| Workbench: Swift build | about 1 |
| Workbench: Swift tests | cut off by the timeout |

Two of the bank UI tests also sat through a 60-second XCUITest "wait for idle" timeout.

### 6. Rust coverage is one serial pile

Everything runs in one `cargo llvm-cov`. Four tests take most of it: `every_ported_package_matches_its_cpython_oracle_under_every_memory_backend` (18.6+ minutes, killed), `workbench_retains_record_identity_through_shared_proof_helpers` (6.7), `workbench_apple_host_includes_shared_proof_dependencies` (3.4) and the first Workbench goal test (1.0).

### 7. Timeouts too long to catch a hang

Limits are 40–90 minutes, and they have no per-test bound. The 90-minute limits sit on 5-minute jobs.

### 8. The Workbench compile

The Workbench compile took 615 seconds on CI at `6119a002`, and takes 125 seconds and 1 GB on this Mac at `d7360f5d`. `8356f2f7` and `d7360f5d` fix a compiler cost here, so re-measure this after the changes below.

## The plan, now applied in `ci.yml`, `gpui.yml`, `ci-windows.yml` and `winui.yml`

No assertion is deleted from the repository's overall coverage: everything removed from Windows or Mac still runs on Linux.

### Phases

| Phase | Jobs | Target |
|---|---|---|
| 1. Build | Compiler artifact only | 3–4m |
| 2. In parallel with build | Analyse (format, clippy, deslop, hawk, guards), extension bundle, Windows, GPUI, WinUI | under 10m for analyse |
| 3. After build | Every suite, in parallel | under 20m each |
| 4. Gate | Coverage thresholds, root CI | under 1m |

### Linux runs everything headless

- GPUI splits into a suites job and a host job.
- All apps compile once, in parallel, for both smoke and self-test.
- The host's target directory is cached.

### Windows and Mac keep only what they uniquely prove

| Job | Keeps | Removes (still runs on Linux) |
|---|---|---|
| GPUI macos | Every app in a real window, Mac archive and string ownership, clippy | Native, UI and example suites, headless smoke and self-test, host `cargo test` |
| GPUI windows | Probe smoke, Flock and Painting in real windows | Native suite, host `cargo test`, Flock headless smoke |
| Windows core | Runtime, compiler, package client, fiber and omnibus smokes, corpus under gc and arc | UI suites, the corpus's repeat at the default backend |
| WinUI | Every app sandboxed in a real window against its goldens, Storage on the C drive, host tests, AOT | Signal Flow and Storage suites |

### Apple splits into parallel jobs

Each uses the cached compiler setup, with its own tight timeout: iOS (host, ABI, corpus, mobile), macOS (hosts, corpus), Talon Bank (simulator UI) and Workbench.

### Rust coverage shards

`cargo llvm-cov` runs in parallel jobs: the Python packages, the Workbench and proof tests, and everything else. The coverage job merges their line counts before checking thresholds, so the floors don't change.

### Tighter timeouts

Each job's limit becomes about 1.5 times its measured healthy time, so a hang fails quickly and by name.

## Targets

| Job | Now | Target |
|---|---:|---:|
| Wall clock | 57 | 20–25 |
| Wait before suites start | 11.7 | 4 |
| GPUI ubuntu | 46.8 | Suites under 5, host under 12 |
| GPUI macos | 23.8 | 12 |
| GPUI windows | 27.5 | 12 with a warm cache |
| Windows core | 14.2 | 8 |
| Apple (longest of four) | 45.6 | 15 |
| Rust (longest shard) | 40+ | 15 |

## Measured: the first run of the new shape

Run [41378280720](https://github.com/Nimblesite/osprey/actions/runs/41378280720), commit `a7eb9848`: **22.7 minutes of wall clock and 223 runner-minutes, down from 57 and 324**. Every job finished inside its timeout, and the coverage thresholds passed on the merged Rust shards.

| Job | Minutes | Finished at |
| --- | --- | --- |
| Build | 6.4 | 8.4 |
| Format & Analyse | 4.1 | 6.0 |
| Apple: Talon Bank | 14.1 | 22.6 |
| Apple: iOS | 13.7 | 22.2 |
| Apple: Workbench | 8.6 | 17.1 |
| Apple: macOS | 4.8 | 13.2 |
| GPUI Windows | 20.7 | 22.4 |
| GPUI Linux host | 17.1 | 18.8 |
| GPUI Mac | 12.6 | 14.3 |
| GPUI Linux suites | 6.4 | 8.1 |
| WinUI | 13.6 | 15.3 |
| Windows core | 6.0 | 7.6 |
| Rust: packages | 9.2 | 17.6 |
| Rust: workbench | 8.6 | 17.0 |
| Rust: rest | 6.3 | 14.7 |
| Package source coverage | 11.3 | 19.8 |
| Proof kernel | 10.9 | 19.3 |
| VS Code extension | 10.3 | 18.7 |
| C runtime | 7.7 | 16.1 |
| Integration | 9.9 (failed) | 18.3 |

The run failed on one job, so it does not yet prove the time of a green run. In Integration the Android emulator died ("device offline") after all six bank instrumentation tests had passed, while their screenshots were being pulled. The steps after it (bank e2e, documentation acceptance, web compiler) were skipped, so a passing Integration runs longer.

This failure predates the new shape: the same "device offline" stopped `80fcc980` and `e13285ae`, the second during the language goldens. The cause is not known yet, so the job now prints kernel kills, memory and disk when it fails.

## Measured: the second run

Run [41380639647](https://github.com/Nimblesite/osprey/actions/runs/41380639647), commit `1609311f`: **21.1 minutes of wall clock and 210 runner-minutes**. It failed on two jobs, so it still does not prove the time of a green run, because the bank e2e, documentation and web compiler steps of Integration were skipped again.

- **Format & Analyse** stopped at the release gate guard, which refused a `|| true` in the new diagnostic step. `772d9c2b` removes it. Every later analysis step was skipped, so this run did not exercise them.
- **Integration** lost the Android emulator again, and this time the host state is on record.

### The Android emulator

All six instrumentation tests passed at 03:16:47. The next adb command, the first screenshot pull, hung for 33 seconds and ended with `error: closed`, then `device offline`, and the emulator's console port refused the action's `emu kill`.

| Finding | Evidence |
| --- | --- |
| It is not host memory or disk | 12 GiB of memory available, 88 KiB of swap used, 73 GB of disk free |
| The kernel did not kill it | `dmesg` shows no out-of-memory kill and no segfault |
| It is not a step time limit | Passing runs lasted 6.4 to 7.9 minutes; this one died at 7.5 |
| The new shape did not introduce it | `80fcc980` and `e13285ae` died the same way before it, and the shape did not change the emulator's options |

The step passed on `c6782c6b`, `6119a002`, `13ffbced`, `d7360f5d` and `50fdef0a`, and failed on `80fcc980`, `e13285ae`, `a7eb9848` and `1609311f`. Both finished runs of the new shape are among the failures, which is too few to say whether the rate has changed.

The cause inside the emulator is still unknown. Its own output says nothing at the moment it dies, so the next thing to capture is the guest's log.
