# Website Build Scripts

Scripts for building the Osprey website.

## Videos

The site's videos, posters, caption tracks and homepage motion are made in the Videos repository, copied here with `make media` from the repository root and committed through Git LFS ([MEDIA-VIDEOS-REPO] in [plan 0059](../../../../docs/plans/0059-video-tutorials.md)). `make -C src/toolchain/website video-metadata` measures them for `src/_data/videos.json`.

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
