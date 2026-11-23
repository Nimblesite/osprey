# lspkit leak-fix upgrade

Bring lspkit's memory-leak fixes into the Osprey language server once lspkit releases them, and fix the Osprey-side leak found in the same review.

## [LSPKIT-UPGRADE-SOURCE] What lspkit is fixing

The lspkit agent on TMC (`codex-memory-leak-fix`) is fixing the five leaks in lspkit's `docs/plans/memory-leak-audit.md`. Osprey pins `lspkit`, `lspkit-server` and `lsp-profiler` at `0.10.1` in `src/toolchain/Cargo.toml`; 0.7.0–0.10.1 add more leak and lifecycle fixes (sampler stop on an abandoned attach, owned forwarder tasks, disposed decoration types).

| lspkit leak | Osprey exposure |
| --- | --- |
| Correlator keeps abandoned requests | None: Osprey does not use the sidecar correlator |
| Dropped inbound readers keep their decoder | None: Osprey reads with `read_message` on its own loop |
| Unknown method keeps a pre-registered ingress control | None today: Osprey calls `Dispatcher::dispatch`, which registers only after the handler is found, and never calls `CancellationHandle::register` |
| Reference server keeps closed documents | None: reference example only |
| Reference server handler cycle | None: reference example only |

The upgrade still matters: Osprey must not stay on a lspkit that has known leaks, and later Osprey code must not pick up the ingress API while it leaks.

## [LSPKIT-UPGRADE-OSPREY-LEAK] The Osprey-side leak

`SourceCache::kept` (`src/toolchain/crates/osprey-lsp/src/project_sources.rs`) keeps every project whose membership contains an open file. Opening one file that many projects list as a source (`testing/src`, a workspace package adopted by `workspace::adopt`) keeps every dependent's parsed sources and analysis for as long as the tab stays open. This defeats the four-project cap. `an_open_shared_file_does_not_pin_its_dependents` in `project_cache.rs` fails on it: `after 4: 6 analyses, 7 files pinned`, and the count keeps growing.

Fix: keep only the projects that loaded the open file (its `project_roots`), plus the most recent ones.

## [LSPKIT-UPGRADE-STEPS] Steps

- [x] Static review of the Osprey LSP's long-lived state; failing test for the leak it found
- [x] Fix `SourceCache::kept`; the new test passes and `the_sweep_releases_closed_projects_and_keeps_open_ones` stays green
- [x] lspkit agent: release its fixes (version above `0.4.0`, published to crates.io) and post the version on TMC to `osprey-lsp-leak`
- [x] Bump `lspkit`, `lspkit-server` and `lsp-profiler` in `src/toolchain/Cargo.toml` to that release; `cargo update -p` each, in one commit
- [x] Fix any API break in `osprey-lsp` and `lsp-profiler-osprey`, with no change to behaviour (none: Osprey uses no changed API)
- [x] Rust pinned to 1.100.0 everywhere ([TOOLCHAIN-RUST-PIN]), the version lspkit builds with
- [x] `live_profiler_lsp` found queued requests ignored `$/cancelRequest`; fixed with `work/pending.rs`
- [x] `cargo test -p osprey-lsp` (261 passed), `-p lsp-profiler-osprey` (21 passed) and `--test live_profiler_lsp` (5 passed), 2026-11-23 at `4f3215fa8` + `0c5725f73`
- [ ] The VS Code extension LSP suite (`make _test_vscode_extension`), then `make ci`
- [ ] Push under the TMC lock `PUSH-<branch>` once `make ci` is green
