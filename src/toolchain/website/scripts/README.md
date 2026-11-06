# Website Build Scripts

Scripts for building the Osprey website.

## Homepage motion

Run `node scripts/render-flight.mjs` from `src/toolchain/website/` to reproduce the original wireframe osprey film and its static poster in `src/assets/motion/`. The geometry lives in `flight-scene.js`; Playwright's Chromium records an eight-second VP9 WebM loop. These authored media assets ship with the site, so normal builds need no rendering step. The browser decodes the film without running its canvas renderer. Playback pauses offscreen and in background tabs; reduced-motion and data-saving preferences use the poster until the visitor requests playback.

## Browser runtime assets

The build publishes Prism and SQLite from the website’s locked npm dependencies, together with their licenses. The WebAssembly studio loads them locally; its browser acceptance test blocks the old CDN and still runs the database workflow.

## Scripts

### `generate-docs.sh`
Regenerates the API reference (`src/docs/`) via `osprey --docs` when a Rust
compiler binary (`../../../../target/release/osprey`, built with `cargo build
--release`) is present and supports the flag. Otherwise the committed docs in
`src/docs/` are used as-is, so the website build never requires a Rust
toolchain.

**Usage:**
```bash
./scripts/generate-docs.sh
```

### `add-ml-twins.py`
Adds ML examples to generated reference pages. Set `mlTwins: manual` in a hand-authored guide to preserve its complete examples; `mlTwins: false` omits generated twins.

### `copy-spec.js`
Publishes the subject folders in `docs/specs/` using their shared `sections.json` metadata. Language and compiler contracts have their own indexes; runtime, platforms, tools, ecosystem and applications are separate. Existing per-document URLs remain stable. Inter-contract links resolve to the published page, while source links point to GitHub. Unclassified folders, flat contract files and colliding URLs fail the build.

### `update-playground.js`
Syncs the playground editor content from the three flavor twins of
`src/osprey/tests/effects/install/install.test.{osp,ospml,py}`.

## Manual Documentation Generation

```bash
cargo build --manifest-path src/toolchain/Cargo.toml --release
./target/release/osprey --docs --docs-dir src/toolchain/website/src/docs
```
