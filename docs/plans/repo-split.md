# Repo split: Package manager, Perception, Osprey Native [REPO-SPLIT]

## Answers

| Question | Answer |
|---|---|
| Split now? | **Yes, two of the three: done.** |
| How many now? | **2: the Package manager ([Nimblesite/osprey-packages](https://nimblesite.github.io/osprey-packages/)) and Perception ([Nimblesite/perception](https://nimblesite.github.io/perception/)).** Both are public. Osprey Native stays until the conditions in [REPO-SPLIT-NATIVE-GATE] are met. |
| Submodules? | **No.** Only CI tests read the split repos: `make downstream` clones each one's `main` and lays its files over this checkout at their original, gitignored paths ([REPO-SPLIT-MAIN]). Nothing else here depends on them. |

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

### Why CI tests clone main, not submodules [REPO-SPLIT-MAIN]

- **No path changes.** Each split repo keeps every file at the path it had here. `make downstream` (`scripts/ci/downstream.sh`) lays those files over this checkout from each repo's `main`. The paths are gitignored, so `daw/osprey.toml`'s `../../packages/native/host` source roots and the compiler-side tests resolve as before.
- **Tests read main.** There is no pins file. Every CI run tests the compiler against each split repo's `main`, so a split repo's change is checked against the compiler at once, and nothing in ospreylang needs a bump.
- **Only tests depend on them.** The build, the releases, the VSIX and the website read nothing from a split repo. Only the canaries, the compiler-side tests, the extension's package tests, the FFT proof and the Windows Prism self-test lay them.
- **No dependency cycle.** Each split repo pins ospreylang by **release tag** in `osprey-version`. Its `scripts/workspace.sh` checks that tag out and lays the repo over it, the reverse of `make downstream`, so its recipes run unchanged. Perception hashes with the package manager's `bytes.osp` and `sha256.osp`, which its `osprey-version` tag (v0.33.0) still carries, so neither split repo reads the other.
- **No keys.** No deploy key, token or secret reads a split repo: all three repos are public, so every checkout is a plain one.

## Repos [REPO-SPLIT-REPOS]

Each repo keeps its history (`git filter-repo --path`), its own CI and its own GitHub Pages site. The specs, designs and messaging docs stay here and link to the product sites.

| Repo | Moved in | Make targets | Website |
|---|---|---|---|
| `Nimblesite/osprey-packages` | `src/osprey/packages/manager/`, `src/toolchain/schemas/package-registry.td`, `src/toolchain/hosts/web/src/registry-{form,catalog}.mjs`, `scripts/ci/ci-registry-contract-tests.mjs`, the registry scripts in `scripts/packages/` | `scripts/make/osprey-packages.mk` | <https://nimblesite.github.io/osprey-packages/> (the registry site, `registry-pages`) |
| `Nimblesite/perception` | `src/osprey/examples/daw/`, `src/toolchain/hosts/synth_vst/` (FerroUI stays here: `src/osprey/packages/ferroui/` is an Osprey Native package and the preferred renderer), `scripts/make/daw-{profile,video}.sh`, `scripts/make/daw-playback-rate.mjs`, `src/toolchain/targets/macos/tests/prism-icon.sh` | `scripts/make/perception.mk` | <https://nimblesite.github.io/perception/> (`daw/website`) |
| `Nimblesite/osprey-native` *(later)* | `src/osprey/packages/native/`, `src/toolchain/osprey-native-website/`, `src/toolchain/schemas/native.td`, the Native hosts | | `osprey-native-website` |

`@osprey/synth` and `@osprey/signal_flow` stay here as standard packages, because synth must never depend on Perception ([DAW]). The package manager is osprey-packages' to ship: neither the releases nor the VSIX carry `osprey-pkg`, `osprey-registry` or the package workspace. The extension reads them from `osprey.packages.frontendPath`, `osprey.packages.appPath` and `osprey.packages.localRegistry.serverPath`. The package hashing test, the local registry stager and `registry-serve` moved to osprey-packages. `daw-clicks.swift` and `sample-attribution.py` moved to Perception.

**Moved, verified** against pre-split `main` (edea7658c): osprey-packages holds all 377 files its `paths` own, and Perception all 186 (now 188 with the two tools). None is still tracked here. The only content changes are the split's own edits: the registry contract test, Perception's Pages path prefix and each repo's new make file.

## Canary tests ospreylang keeps [REPO-SPLIT-CANARY]

Each split repo has **one** `make canary` target holding its major end-to-end tests. Ospreylang calls it (`make _canaries`) and never copies its recipe.

| Repo | `make canary` |
|---|---|
| osprey-packages | The fixture and schema suite, the package frontend suite and the `e2e` consumer flow against `sample-registry` |
| perception | The Prism synth's host-free journeys, the song renderer under ARC and, on macOS, the daw replaying `smoke.jsonl` in the GPUI host |

ospreylang CI runs, on every build, from each split repo's `main`:

- `test-canary` (Linux): both canaries, the package-consumers contract, and every project built on `@osprey/synth`, the laid daw included.
- `test-apple`'s Perception part: both canaries on macOS, so the GPUI smoke runs.
- `ci-windows`: both canaries on Windows (`make _canaries` under MSYS2).
- `winui`: Prism's `synth.dll` built and self-tested on Windows with the other apps.
- `test-rust`: the compiler-side tests that use the split code, unchanged: the editor's wasm boundary build, the daw song renders, and the ambient builtin inventory against the registry's `surface.osp`.
- `build` and `test-extension`: the package client, registry server, package workspace and registry design, staged only for the extension's package tests (`_vsix_bundle_packages`, `_vsix_package_design`, `VSIX_PACKAGES=1`).
- `test-proof`: the production FFT proof over the daw's `arrange_math.ospml`.

These were **removed from ospreylang CI** and run only in their own repo: `build-registry-hosted`, `test-registry-gc`, `test-registry-arc` and `test-registry-hosted` (now osprey-packages' `build`, `test` and `test-hosted`), the GPUI DAW part, and the daw, Prism and VST targets in the platform Makefiles.

## Cross-repo breaks [REPO-SPLIT-BREAKS]

- **An ospreylang PR turns a canary red:** fix it on the split repo's `main` first, then rerun the ospreylang PR. Merging red is never allowed.
- **A split repo needs a compiler fix:** fix it in ospreylang, release it, then bump `osprey-version` in the split repo.
- `osprey-version` is bumped by hand on each ospreylang release. There is no scheduled bump workflow, which saves spend.

## Checklist

- [x] Create `Nimblesite/osprey-packages` (private) with filter-repo history, its own CI, Git LFS and the registry site on GitHub Pages
- [x] Add a `make canary` target to osprey-packages
- [x] Create `Nimblesite/perception` (private) with filter-repo history, its own CI, Git LFS (135 objects) and its website on GitHub Pages
- [x] Add a `make canary` target to perception
- [x] In ospreylang, delete the moved files, add `make downstream` (each repo's `main`) and `make canary`, delete the registry jobs, and run the canaries on Linux, macOS and Windows
- [x] Point the docs here at the product websites; the specs and designs stay here
- [x] Both split repos build, test and gate coverage keyless: Perception reads no private repository
- [x] Make both repos public
- [x] Nothing but CI tests depends on the split repos: releases and the VSIX ship no package manager, and the website links instead of copying
- [ ] Merge `repo-split` (PR #439) on green CI
- [ ] Bump each split repo's `osprey-version` to the first release without the moved files, so its workspace stops carrying ospreylang's old copies; Perception then names `Nimblesite/osprey-packages` in `requires` again for `bytes.osp` and `sha256.osp`, read keyless now that it is public
- [x] Give each split repo the three CI phases, with its own `coverage-thresholds.json` and coverage gate (osprey-packages 91.1%, Perception 85.4%, merged by `scripts/coverage.mjs` over the repo's own files)
- [x] Run `_check_programs` over each split repo's programs in its own CI (the Build job narrows the index to the repo's `paths`)
- [ ] Check each Native gate condition ([REPO-SPLIT-NATIVE-GATE]), then repeat these steps for `Nimblesite/osprey-native`, whose canary also runs `perception`'s `make canary`
