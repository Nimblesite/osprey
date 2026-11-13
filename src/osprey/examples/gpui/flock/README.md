# Flock

Flock is an arcade herding game written entirely in Osprey with `@osprey/native`. Steer the sheepdog, guide boids into the translucent pen on the right, and avoid the hawks. Rescued birds score one point. Each cleared round adds 500 birds and one hawk, up to 20,000 birds and six hawks; the best five scores persist in `flock-scores.json`.

## Controls

- **Arrow keys:** move the sheepdog while held.
- **Pointer:** hold to steer toward a position in the field.
- **P** or **Escape:** pause or resume.
- **Settings:** choose the flock size for the next round. Your best five rounds are kept between runs.

## Run

From the repository root:

```sh
make -C src/toolchain/targets/macos flock                                          # 1,000 birds
make -C src/toolchain/targets/windows flock                                        # Windows: the same, sandboxed on the WinUI host
bash src/osprey/examples/gpui/flock/run.sh --flock 5000       # any size from 1 to 20,000
```

## How it stays fast

- **Resident session.** [`main.ospml`](main.ospml) keeps the framework's `Surface` in a file-scope cell between the host's `argument`, `open` and `event` calls, so a frame never serializes or parses the flock ([NATIVE-SHM-BOUNDARY](../../../packages/native/README.md)).
- **Shared frame.** The field, HUD and panel cross as the view rows the host reads in place, and each bird's disc rides beside them as a scene row of the canvas it belongs to; nothing is encoded in either direction ([NATIVE-SHM-VIEW](../../../packages/native/README.md), [NATIVE-SHM-SCENE](../../../packages/native/README.md)).
- **Bounded steering.** [`steering.ospml`](src/steering.ospml) reads the flock as float lists sorted by cell, and each bird steers by its first seven flockmates within eight units, the topological rule real starling flocks follow (Ballerini et al., PNAS 2008). The walk carries its sums as arguments and allocates once per bird; alternate halves of the flock look for flockmates each frame.

## Test

```sh
./target/release/osprey src/osprey/examples/gpui/flock/test --run --memory=arc
```

The tests drive the typed host event boundary every host calls ([NATIVE-SHM-EVENT](../../../packages/native/README.md)), including movement, rescue, predation, pause, rounds, the scene the field drew, score loading, launch sizes and the steering rules.

## Measured

`run.sh --bench 10 --flock N` on an Apple M4 Max MacBook Pro (120 Hz display), measured of the resident build before the shared-memory boundary. `frame` is main-thread time from a frame event to the end of the paint that shows it: dispatch, layout and paint. A 120 Hz frame has 8.3 ms.

| Flock | Frame | Dispatch | Paint |
| --- | --- | --- | --- |
| 1,000 birds | 1.7 ms | 1.2 ms | 0.05 ms |
| 5,000 birds | 4.7 ms | 4.0 ms | 0.2 ms |

The window's own `fps` follows the display clock, which macOS slows to about 26 Hz while the screen is locked or the window is hidden; the frame cost above does not depend on it. Before the resident session and the scene rows, 5,000 birds cost 29 ms of dispatch and ran at 27 fps.
