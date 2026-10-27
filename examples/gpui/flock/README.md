# Flock

Flock is an arcade herding game written entirely in Osprey with `@osprey/native`. Steer the sheepdog, guide boids into the translucent pen on the right, and avoid the hawks. Rescued birds score one point. Each cleared round adds 500 birds and one hawk, up to 20,000 birds and six hawks; the best five scores persist in `flock-scores.json`.

## Controls

- **Arrow keys:** move the sheepdog while held.
- **Pointer:** hold to steer toward a position in the field.
- **P** or **Escape:** pause or resume.
- **Settings:** choose the flock size for the next round and toggle sound.

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

| Machine | Flock | FPS |
| --- | --- | --- |
