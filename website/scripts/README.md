# Website Build Scripts

Scripts for building the Osprey website.

## Homepage motion

Run `node scripts/render-flight.mjs` from `website/` to reproduce the original wireframe osprey film and its static poster in `src/assets/motion/`. The geometry lives in `flight-scene.js`; Playwright's Chromium records an eight-second VP9 WebM loop. These authored media assets ship with the site, so normal builds need no rendering step. The browser decodes the film without running its canvas renderer. Playback pauses offscreen and in background tabs; reduced-motion and data-saving preferences use the poster until the visitor requests playback.

## Scripts

### `generate-docs.sh`
Regenerates the API reference (`src/docs/`) via `osprey --docs` when a Rust
compiler binary (`../target/release/osprey`, built with `cargo build
--release`) is present and supports the flag. Otherwise the committed docs in
`src/docs/` are used as-is, so the website build never requires a Rust
toolchain.

**Usage:**
```bash
./scripts/generate-docs.sh
```

### `copy-spec.js`
Copies the language specification from `docs/specs/` to the website source.

### `update-playground.js`
Syncs the playground editor content from
`tests/regressions/basics/osprey_mega_showcase.test.osp`.

## Manual Documentation Generation

```bash
cargo build --release
./target/release/osprey --docs --docs-dir website/src/docs
```
