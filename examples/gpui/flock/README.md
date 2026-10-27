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
OSPREY_GPUI_APP="$PWD/examples/gpui/flock" hosts/gpui/run.sh
```

## Test

```sh
./target/release/osprey examples/gpui/flock/test --run --memory=arc
```

The tests drive the real serialized `Runtime::start` and `Runtime::dispatch` boundary, including movement, rescue, predation, pause, rounds, scene encoding and score loading.

## Measured

`run.sh --bench 20`, window focused (macOS throttles the display clock of a window behind another):

| Machine | Flock | FPS | Dispatch | Paint | Envelope |
| --- | --- | --- | --- | --- | --- |
| Apple M-series MacBook Pro, 120 Hz display | 1,000 birds | 107 to 120 | 5.8 to 7.3 ms | 0.3 ms | 90 to 140 KB |

Without the window, 2,000 birds cost 21 ms and 5,000 birds 45 ms per frame through start and dispatch, so the 5,000-bird gate in [plan 0036](../../../docs/plans/0036-gpui-desktop-host.md) is not met yet.
