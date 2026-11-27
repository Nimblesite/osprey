# Repo split: Package manager, Perception, Osprey Native [REPO-SPLIT]

## Answers

| Question | Answer |
|---|---|
| Split now? | **Yes, two of the three: done.** |
| How many now? | **2: the Package manager ([Nimblesite/osprey-packages](https://nimblesite.github.io/osprey-packages/)) and Perception ([Nimblesite/perception](https://nimblesite.github.io/perception/)).** Both are private. Osprey Native stays until the conditions in [REPO-SPLIT-NATIVE-GATE] are met. |
| Submodules? | **No.** ospreylang pins each split repo's commit in `scripts/ci/downstream.json`, and `make downstream` lays its files over this checkout at their original, gitignored paths ([REPO-SPLIT-PIN]). |

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

### Why pinned commits, not submodules [REPO-SPLIT-PIN]

- **No path changes.** Each split repo keeps every file at the path it had here. `make downstream` (`scripts/ci/downstream.sh`) lays those files over this checkout at the pinned commits. The paths are gitignored, so `daw/osprey.toml`'s `../../packages/native/host` source roots, the package client build and the compiler-side tests resolve as before.
- **The pin is atomic.** `scripts/ci/downstream.json` records exactly which commit ospreylang tests against. A compiler PR that breaks a canary fixes the split repo and bumps the pin in the same PR.
- **A public repo cannot carry a private submodule.** Every clone and fork would fail on it. A pin file costs nothing to anyone who never runs `make downstream`.
- **No dependency cycle.** Each split repo pins ospreylang by **release tag** in `osprey-version`. Its `scripts/workspace.sh` checks that tag out and lays the repo over it, the reverse of `make downstream`, so its recipes run unchanged. Perception also pins the osprey-packages commit it hashes with, in `requires`.
- **Private access.** CI reads each split repo with a read-only deploy key, held as the `OSPREY_PACKAGES_DEPLOY_KEY` and `PERCEPTION_DEPLOY_KEY` secrets (Actions and Dependabot). A fork's pull request has no secrets, so its laid jobs fail.

## Repos [REPO-SPLIT-REPOS]

Each repo keeps its history (`git filter-repo --path`), its own CI and its own GitHub Pages site. The specs, designs and messaging docs stay here and link to the product sites.

| Repo | Moved in | Make targets | Website |
|---|---|---|---|
| `Nimblesite/osprey-packages` | `src/osprey/packages/manager/`, `src/toolchain/schemas/package-registry.td`, `src/toolchain/hosts/web/src/registry-{form,catalog}.mjs`, `scripts/ci/ci-registry-contract-tests.mjs`, the registry scripts in `scripts/packages/` | `scripts/make/osprey-packages.mk` | <https://nimblesite.github.io/osprey-packages/> (the registry site, `registry-pages`) |
| `Nimblesite/perception` | `src/osprey/examples/daw/`, `src/toolchain/hosts/synth_vst/`, `src/toolchain/hosts/ferroui/` (only the daw uses it), `scripts/make/daw-{profile,video}.sh`, `scripts/make/daw-playback-rate.mjs`, `src/toolchain/targets/macos/tests/prism-icon.sh` | `scripts/make/perception.mk` | <https://nimblesite.github.io/perception/> (`daw/website`) |
| `Nimblesite/osprey-native` *(later)* | `src/osprey/packages/native/`, `src/toolchain/osprey-native-website/`, `src/toolchain/schemas/native.td`, the Native hosts | | `osprey-native-website` |

`@osprey/synth` and `@osprey/signal_flow` stay here as standard packages, because synth must never depend on Perception ([DAW]). The VS Code extension and every release still ship `osprey-pkg`, `osprey-registry` and the package workspace, built from the pinned osprey-packages commit.

## Canary tests ospreylang keeps [REPO-SPLIT-CANARY]

Each split repo has **one** `make canary` target holding its major end-to-end tests. Ospreylang calls it (`make _canaries`) and never copies its recipe.

| Repo | `make canary` |
|---|---|
| osprey-packages | The fixture and schema suite, the package frontend suite and the `e2e` consumer flow against `sample-registry` |
| perception | The Prism synth's host-free journeys, the song renderer under ARC and, on macOS, the daw replaying `smoke.jsonl` in the GPUI host |

ospreylang CI runs, on every build:

- `test-canary` (Linux): both canaries, the package-consumers contract, and every project built on `@osprey/synth`, the laid daw included.
- `test-apple`'s Perception part: both canaries on macOS, so the GPUI smoke runs.
- `winui`: Prism's `synth.dll` built and self-tested on Windows with the other apps.
- `test-rust`: the compiler-side tests that use the split code, unchanged: the editor's wasm boundary build, the daw song renders, and the ambient builtin inventory against the registry's `surface.osp`.
- `build`, `ci-windows` and `release`: the package client, the local registry and the package workspace.
- `test-proof`: the production FFT proof over the daw's `arrange_math.ospml`.
- `test-website` and the Pages deploy: the registry model and the effect-test excerpt the docs quote verbatim.

These were **removed from ospreylang CI** and run only in their own repo: `build-registry-hosted`, `test-registry-gc`, `test-registry-arc` and `test-registry-hosted` (now osprey-packages' `build`, `test` and `test-hosted`), the GPUI DAW part, and the daw, Prism and VST targets in the platform Makefiles.

## Cross-repo breaks [REPO-SPLIT-BREAKS]

- **An ospreylang PR turns a canary red:** fix it in the split repo first, then bump `scripts/ci/downstream.json` in the ospreylang PR. Merging red is never allowed.
- **A split repo needs a compiler fix:** fix it in ospreylang, release it, then bump `osprey-version` in the split repo.
- `osprey-version` is bumped by hand on each ospreylang release. There is no scheduled bump workflow, which saves spend.

## Checklist

- [x] Create `Nimblesite/osprey-packages` (private) with filter-repo history, its own CI, Git LFS and the registry site on GitHub Pages
- [x] Add a `make canary` target to osprey-packages
- [x] Create `Nimblesite/perception` (private) with filter-repo history, its own CI, Git LFS (135 objects) and its website on GitHub Pages
- [x] Add a `make canary` target to perception
- [x] In ospreylang, delete the moved files, pin both repos in `scripts/ci/downstream.json`, add `make downstream` and `make canary`, delete the registry jobs, and run the canaries on Linux, macOS and Windows
- [x] Point the docs here at the product websites; the specs and designs stay here
- [ ] Add the read-only deploy keys and their `OSPREY_PACKAGES_DEPLOY_KEY` and `PERCEPTION_DEPLOY_KEY` secrets (Actions and Dependabot): every laid CI job fails until they exist
- [ ] Bump each split repo's `osprey-version` to the first release without the moved files, so its workspace stops carrying ospreylang's old copies
- [ ] Give each split repo the three CI phases, with its own `coverage-thresholds.json` and coverage gate
- [ ] Run `_check_programs` over each split repo's programs in its own CI
- [ ] Check each Native gate condition ([REPO-SPLIT-NATIVE-GATE]), then repeat these steps for `Nimblesite/osprey-native`, whose canary also runs `perception`'s `make canary`
