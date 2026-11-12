# Tally

The smallest Osprey Native application with no JSON bridge. Its whole interface (layout, text, both colour schemes) is one pure Osprey function, and it reaches the host as rows in memory the host reads in place. Read [shared-memory views](../../../../docs/specs/applications/0053-NativeSharedViews.md) for how and why.

| File | What it shows |
| --- | --- |
| [`src/app.ospml`](src/app.ospml) | The store, the update function and `Screen::view`, where every colour, size and space is decided. |
| [`main.ospml`](main.ospml) | The boundary: `osprey_app_start()` and `osprey_app_event(ordinal, text)`. The store never leaves the library. |
| [`host/reference.c`](host/reference.c) | A complete host in plain C: it commits each frame, leases it and reads the controls straight from memory. It is also the end-to-end test. |
| [`test/main.ospml`](test/main.ospml) | The application's own tests, with no host. |

## Run it

```sh
make -C src/toolchain/targets/tests tally                  # the application's tests
make -C src/toolchain/targets/macos/tests tally-headless   # the reference host, end to end
```

The second command ends with `[ok] tally reference host: 9 views read in place from shared memory`.

## Hosts

The reference host is the only host today. The SwiftUI and WinUI view hosts that draw these rows as native controls are in progress ([plan 0037, section 11](../../../../docs/plans/0037-osprey-native.md)); neither holds any style, so Tally is restyled by editing `src/app.ospml` alone.
