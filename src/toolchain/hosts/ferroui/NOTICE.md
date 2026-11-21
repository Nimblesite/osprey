# NOTICE

This host is MIT-licensed like the rest of Osprey (Copyright (c) 2025-2026 NIMBLESITE PTY LTD;
see the repository's `LICENSE`). It contains and depends on work by others, credited here.

## FerroUI

- Project: <https://github.com/wieslawsoltes/FerroUI>
- Copyright (c) Wiesław Šoltés
- Licence: MIT, reproduced in full in [LICENSE-FerroUI](LICENSE-FerroUI).

This host began as FerroUI's `ferroui-osprey-native` crate, written by Wiesław Šoltés and merged
into FerroUI in [PR #967](https://github.com/wieslawsoltes/FerroUI/pull/967) ("Osprey Native host
on FerroUI"). The mapping of Osprey Native's node kinds to FerroUI controls (`view.rs`), the
theme and symbol tables (`theme.rs`, `symbols.rs`), the window and frame clock
(`desktop.rs`) and the scene control (`surface.rs`) are derived from that crate, then moved onto
Osprey's shared-memory boundary. Its shared-memory boundary modules (`ffi.rs`, `frame.rs`, `rows.rs`,
`tree.rs`, `regions.rs`, `events.rs`, `store.rs`, `adapters/`, the scene types in `scene.rs`) come from
Osprey's own GPUI host. The `.rs` files are kept exactly as they
are on the `prism-synth` branch of the FerroUI port (commit `e466c116`, on FerroUI `main` at `0382c045`). The host links the FerroUI crates `ferroui-base`,
`ferroui-controls` and `ferroui-view` (and, through them, the rest of FerroUI) at the commit
pinned in `Cargo.toml`.

The FerroUI MIT licence requires its copyright notice and permission notice to be included in all
copies or substantial portions of the software: they are in `LICENSE-FerroUI`, and that file
ships with every copy of this host.

## FerroUI's notice for this crate, verbatim

`ferroui-osprey-native` is a host for applications written with Osprey Native, the application
framework of the Osprey language. It implements the protocol those applications speak (the
envelope, the node kinds, the scene primitives, the events and the capabilities) and follows the
design of Osprey's GPUI host, from their published specifications and sources at the release
v0.29.0 (commit `19ab5d4f231fcd20b016704606d9527645aa288f`):

- `docs/specs/platforms/0046-GpuiDesktopHost.md` and `docs/specs/applications/0047-OspreyNativeHosts.md`;
- `src/osprey/packages/native` (`@osprey/native`: the node, encoder, message and capability types);
- `src/toolchain/hosts/gpui` (the GPUI host).

The shared-memory boundary modules (`ffi`, `frame`, `rows`, `tree`, `regions`, `events`,
`adapters`, `store` and the scene types) are ported from Osprey's GPUI host under its MIT licence
below. The FerroUI mapping (`host`, `view`, `surface`, `desktop`, `bench`) is written for FerroUI.

## Osprey

- Project: <https://github.com/Nimblesite/osprey>
- License: MIT

```
Released under MIT License

Copyright (c) 2025-2026 NIMBLESITE PTY LTD, Australia

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction,
including without limitation the rights to use, copy, modify, merge, publish, distribute,
sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or
substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT
OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```
