# Website Build Scripts

Scripts for building the Osprey website.

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
