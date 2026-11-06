# Verified documentation captures [DOCS-VISUAL]

Captured on 3 November 2026. Screens show running tools or their real saved output.

| Assets | Evidence and reproduction |
| --- | --- |
| python-functions, python-records, python-effects | The first three complete Python-flavor examples in docs/python/flavor.md, compiled natively; output checked before capture. |
| python-hello, python-strings, python-lists | Osprey Python-flavor greeting, interpolation/byte length and iterator-comprehension programs; native output is respectively a greeting, Osprey: 6 bytes, and 0,1,2. |
| python-type-error, python-effect-error | Deliberately rejected programs: integer-plus-string and an unhandled Log.write effect. The compiler exits 1. |
| product-host, product-metal | Identical 2 × 2 matrix product source on host and Apple M4 Max Metal. Both return first element 7.0. Both the product and element read handle Result. This is an execution check, not a performance benchmark. |
| first-app-run, first-app-json | Complete tutorial source, assembled from its six Default-flavor blocks; check succeeds without warnings. Two native runs save runs=2.0, ready=true and tags=[cli,adt]. JSON image is the actual file in Chrome's pretty-print viewer. The tutorial's missing-file policy qualification remains beside its code. |
| hunt-crash, hunt-finding | src/osprey/tests/hunt/crash.osp, one input byte and ten-second budget. The deliberately planted C abort reproduces with input 9; expected exit status 1. |
| hunt-clean | src/osprey/tests/hunt/clean.osp with the same budget: six completed paths, no findings. Bounded clean control, not a correctness proof. |
| hunt-limits | src/osprey/tests/hunt/process.osp: unmodelled pipe operation, no native replay. Engine limitation control, not a reproduced crash. |
| effects-python-doubles, effects-unhandled, effects-resume | Captured 6 November 2026 from the Playground UI with runs routed to the native compiler. The Python-flavor Files test doubles, the Default Files example with no handler and the Shop resume example, all copied from docs/algebraic-effects.md. Output is the compiler's real stdout or diagnostic. |
| python-flavor-static-types | Captured 6 November 2026 from the Playground UI with runs routed to the native compiler: the mixed-type list example in docs/python/flavor.md. The diagnostics are the compiler's real output; CPython runs the same file and prints Ada scored 3.5. |
| api-index, api-function | Real compiler HTML export of src/osprey/packages/json, midnight theme; library landing page and ui::Doc::string. |
| storage-macos, storage-ios | Frames at 12 and 174 seconds of the published Swift interop tutorial, showing recorded native app footage. |
| mathlib-check, mathlib-library | Frames at 80 and 116 seconds of the published Mathlib bridge tutorial. Historical footage; guide qualifications about imported proofs and assumptions apply. |
| proof-workbench-commutativity | A frame at 0:33 of the real recording osprey-proof-workbench-math.mp4, cropped to the Proof Workbench window: Commutativity of conjunction, checked by the Osprey kernel. |

The Playground captures use the real site UI and a local adapter that sends the exact editor source to the native Osprey compiler, rather than to the hosted service. Success stdout and compiler errors come from that execution; rejected programs return HTTP 400 to preserve the UI's compilation-failure state. This does not demonstrate browser-side compilation. No output, program result or model response was fabricated.

A stale release compiler initially called the old five-argument matrix ABI against the updated six-argument runtime. That run returned Error and was not captured as success. The current debug compiler and runtime produced the host and Metal results above. The source in the captures handles failures explicitly.

## Rules

- A guide of 500 prose words or more carries at least one illustration per 500 words. This is the project's editorial target, not a search rule; a browser test enforces it. Video-card posters don't count, and short reference entries and generated function pages are outside it.
- No screenshot is a mockup, and none promises universal host conformance. Keep the security qualifications and feature limits a capture shows.
- After changing a video asset, run `make -C src/toolchain/website video-metadata`: `VideoObject` durations come from the measured file, and upload dates stay as originally published.
