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
make gpui-flock                                    # 1,000 birds
bash examples/gpui/flock/run.sh --flock 5000       # any size from 1 to 20,000
```

## How it stays fast

- **Resident session.** [`main.ospml`](main.ospml) keeps the framework's `Session` in a file-scope cell and calls `Runtime::begin` and `Runtime::proceed`, so a frame never serializes or parses the flock ([UI-RESIDENT](../../../packages/native/README.md)).
- **Scene stream.** Each bird's disc goes to the host through the `gpui_shape` import instead of the envelope, which stays under 2 KB a frame ([UI-SCENE-STREAM](../../../packages/native/README.md)).
- **Bounded steering.** [`steering.ospml`](src/steering.ospml) reads the flock as float lists sorted by cell, and each bird steers by its first seven flockmates within eight units, the topological rule real starling flocks follow (Ballerini et al., PNAS 2008). The walk carries its sums as arguments and allocates once per bird; alternate halves of the flock look for flockmates each frame.

## Test

```sh
./target/release/osprey examples/gpui/flock/test --run --memory=arc
```

The tests drive the real serialized `Runtime::start` and `Runtime::dispatch` boundary, including movement, rescue, predation, pause, rounds, scene encoding, score loading, launch sizes and the steering rules.

## Measured

`run.sh --bench 10 --flock N` on an Apple M4 Max MacBook Pro (120 Hz display). `frame` is main-thread time from a frame event to the end of the paint that shows it: dispatch, envelope apply, layout and paint. A 120 Hz frame has 8.3 ms.

| Flock | Frame | Dispatch | Paint | Envelope |
| --- | --- | --- | --- | --- |
| 1,000 birds | 1.7 ms | 1.2 ms | 0.05 ms | 1.7 KB |
| 5,000 birds | 4.7 ms | 4.0 ms | 0.2 ms | 1.7 KB |

The window's own `fps` follows the display clock, which macOS slows to about 26 Hz while the screen is locked or the window is hidden; the frame cost above does not depend on it. Before the resident session and the scene stream, 5,000 birds cost 29 ms of dispatch and ran at 27 fps.
