# Repo split: Package manager, Perception, Osprey Native [REPO-SPLIT]

## Answers

| Question | Answer |
|---|---|
| Split now? | **Yes, two of the three now.** |
| How many now? | **2: the Package manager and Perception.** Osprey Native stays until the conditions in [REPO-SPLIT-NATIVE-GATE] are met. |
| Submodules? | **Yes, for now, one way only:** ospreylang mounts each split repo as a submodule **at its current path**. Split repos never add ospreylang as a submodule. Submodules are removed in [REPO-SPLIT-EXIT]. |

### Goals [REPO-SPLIT-GOALS]

1. **Cut the repo's size.** At over 600k lines of code, it has become unwieldy.
2. **Give each product its own identity:** its own repo, messaging, website and release.
3. **Cut CI time.** Ospreylang CI takes too long. Each product's full suite moves out, and only its canaries ([REPO-SPLIT-CANARY]) stay.

Measured with `git ls-files | xargs cat | wc -l`, the moved code is: Perception about **171k lines** (including `synth_vst`), the manager about **42k**, and Native about **11k**. Splitting the two large products now delivers about 95% of the size reduction available from all three.

### Why split the manager and Perception now [REPO-SPLIT-WHY-NOW]

- **Nothing depends on them.** No crate, host or example imports `src/osprey/packages/manager` (161 MB) or `src/osprey/examples/daw` (64 MB). They only use the compiler and the stdlib packages, so moving them out breaks nothing above them.
- **They deploy on their own.** The manager ships to Supabase and GCP. Perception ships as a VST3, a DAW and device images. Neither release has to wait for the compiler's release.
- **Less CI spend.** Today every ospreylang PR runs `build-registry-hosted` plus `test-registry-gc/arc/hosted`, and the full DAW suites on every platform. After the split, ospreylang runs only the canaries ([REPO-SPLIT-CANARY]).

### Why Native waits [REPO-SPLIT-NATIVE-GATE]

Native is small (about 11k lines), so splitting it now would barely cut the repo's size or CI time. Its tests would still run here, because every app example uses it. Native is also not a leaf. **19 example apps** load `../../packages/native/...` through `source_roots`, and **15 files** in `src/toolchain/crates` and `src/toolchain/hosts` (GPUI, WinUI, Swift, Android, FerroUI) depend on it. Splitting it now would turn every app example and host into a cross-repo change. Native splits when all three of these hold:

1. Every example depends on `@osprey/native` from the registry (`[dependencies]`), not on a relative `source_roots` path.
2. The compiler-side Native code is generated from `src/toolchain/schemas/native.td` and is the only interface the hosts link against.
3. The Perception split has run for 4 weeks without a cross-repo red build on `main`.

Until then, ospreylang CI covers Native. Native's rule that it must pass the Perception canary is met by ospreylang running that canary.

### Why submodules, and only one way [REPO-SPLIT-SUBMODULE]

- **No path changes.** `daw/osprey.toml` (`../../packages/native/host` and the others), `scripts/make/apps.mk` (`UI_SUITES`, the `--smoke` loop), `winui.yml` and the targets Makefiles keep working as they are.
- **The pin is atomic.** The submodule SHA records exactly which commit ospreylang tests against. A compiler PR that breaks a canary fixes the split repo and bumps the pointer in the same PR.
- **No dependency cycle.** Split repos pin ospreylang by **release tag** in `osprey-version`. Their CI downloads that release's `osprey` binary and checks out the same tag for the stdlib package sources. It never builds the compiler, so it costs no compiler build minutes.

## Repos [REPO-SPLIT-REPOS]

Each repo follows the AgentPMO layout and Shipwright deployment. Each keeps the three CI phases (build+check, test with coverage, coverage gate) with its own `coverage-thresholds.json`, and has its own `make ci`. History is kept with `git filter-repo --path <p> --path <p2>`.

| Repo | Moves in | Messaging doc | Website |
|---|---|---|---|
| `Nimblesite/osprey-packages` | `src/osprey/packages/manager/`, `scripts/packages/`, `scripts/ci/ci-registry-contract-tests.mjs`, `src/toolchain/schemas/package-registry.td`, `docs/specs/ecosystem/0030-*`, `0044-*`, `docs/plans/0020-package-manager.md`, `src/toolchain/website/src/assets/images/packages/` | **New** `docs/messaging/osprey-packages.md` (written in the split PR; none exists today) | `manager/web/site` (the registry site) |
| `Nimblesite/perception` | `src/osprey/examples/daw/` (including `website/`, `PRISM.md`), `src/toolchain/hosts/synth_vst/` (moves to its own Cargo workspace), `docs/specs/synth/Perception.md`, `docs/designs/prism-timeline/` | `docs/messaging/perception messaging.md` → `docs/messaging/perception.md` | `daw/website` |
| `Nimblesite/osprey-native` *(later)* | `src/osprey/packages/native/`, `src/toolchain/osprey-native-website/`, `src/toolchain/schemas/native.td`, the Native hosts | `docs/messaging/osprey-native.md` | `osprey-native-website` |

`@osprey/synth` and `@osprey/signal_flow` stay in ospreylang as stdlib packages, because synth must never depend on Perception ([DAW]). The ospreylang website keeps one page per product that links out, and the deploy pages for the moved sites are deleted from `deploy-pages.yml`.

## Canary tests ospreylang keeps [REPO-SPLIT-CANARY]

Each split repo exposes **one** make target, `make canary`, containing its major end-to-end tests. Ospreylang (and later osprey-native) calls that target and never copies its recipe. Canary jobs check out only their own submodule (`git submodule update --init <path>`). Every other job uses `submodules: false`.

| Repo | `make canary`, on Linux, macOS and Windows |
|---|---|
| osprey-packages | `manager/e2e` consumer flow (`osprey init → add → install → build → run` against `sample-registry`) |
| perception | `daw/test` headless suite (`_app-suite`), `--smoke` launch with a golden screenshot comparison, and the Windows `synth.dll` build plus WinUI smoke (today's `winui.yml` steps) |

These are **removed from ospreylang CI** and run only in their own repo: `build-registry-hosted`, `test-registry-gc`, `test-registry-arc`, `test-registry-hosted`, the auth, database, governance and hosted test suites, `daw-profile`, `daw-video`, `daw-playback-rate`, the `synth-vst-*` targets, device builds, and track renders. `_check_programs` ([CI-PROGRAMS-COMPILE]) for the moved programs also moves to their repos.

## Cross-repo breaks [REPO-SPLIT-BREAKS]

- **An ospreylang PR turns a canary red:** fix it in the split repo first, then bump the submodule pointer in the ospreylang PR. Merging red is never allowed.
- **A split repo needs a compiler fix:** fix it in ospreylang, release it, then bump `osprey-version` in the split repo.
- `osprey-version` is bumped by hand on each ospreylang release. There is no scheduled bump workflow, which saves spend.

## Exit from submodules [REPO-SPLIT-EXIT]

Each submodule is removed from ospreylang once its repo uses only registry dependencies (no `../../packages` paths). The canary job then checks out the split repo at the SHA recorded in `ci/downstream.json`.

## Checklist

- [ ] Write `docs/messaging/osprey-packages.md`
- [ ] Create `Nimblesite/osprey-packages` with filter-repo history, its own CI, Supabase/GCP deploy and registry site
- [ ] Add a `make canary` target to osprey-packages
- [ ] In ospreylang, replace `packages/manager` with a submodule, delete the registry jobs from `ci.yml`, and add the canary job on Linux, macOS and Windows
- [ ] Create `Nimblesite/perception` with filter-repo history, its own CI and website, and the `synth_vst` workspace
- [ ] Add a `make canary` target to perception
- [ ] In ospreylang, replace `examples/daw` with a submodule, trim `UI_SUITES`, the smoke loop and `winui.yml` to the canary, and delete the moved targets from the targets Makefiles
- [ ] Move the messaging docs and specs, and fix every link and spec ID cross-reference
- [ ] Check each Native gate condition ([REPO-SPLIT-NATIVE-GATE]), then repeat these steps for `Nimblesite/osprey-native`, whose canary also runs `perception`'s `make canary`
