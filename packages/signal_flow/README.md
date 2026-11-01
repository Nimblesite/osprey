# @osprey/signal-flow

A signal-flow graph editor whose whole interface is data ([UI-MACHINE-FLOW](../../docs/specs/platforms/UiMachines.md)). [signal_flow.machine.json](signal_flow.machine.json) holds the inputs, the `commands` and `menu` layers, a native fallback view and four canvas skins. `ui::Machines` and `ui::Templates` in [@osprey/native](../native/README.md) play it on any host. The only code is the adapter, [src/flow.ospml](src/flow.ospml). It evaluates the signal path and keeps the undo history.

| Skin | Design | Nodes: control, source, process, output, monitor |
| --- | --- | --- |
| Topology | [design/topology.png](design/topology.png) | Policy, Client, Gateway, Service, Telemetry |
| Notation | [design/notation.png](design/notation.png) | k, x(t), H(s), y(t), Scope |
| Paper | [design/paper.png](design/paper.png) | Parameter, Source, Transform, Output, Monitor |
| Prism | [design/prism-audio-modulation.png](design/prism-audio-modulation.png) | LFO, Oscillator, Filter, Master, Scope |

On the canvas you can switch any node off (bypass it), drag the control and process parameters on the node or in the inspector, select a node to inspect its connections, health and parameter, undo and redo, and change skin from the title menu. Health and readouts such as throughput come from the adapter's evaluation: level = source × process × control, where a bypassed process passes its input through.

## Run

```sh
make -C apps/macos signal-flow                    # or: bash packages/signal_flow/app/run.sh
make -C apps/windows signal-flow                  # Windows: the WinUI sandbox
```

Pass `--skin notation` (or `paper`, `prism`) after the script to start in another skin.

## Skins

`skins/spec.py` records where every live part sits in each design, in mock pixels. `skins/build.py` slices each mock into `skins/<skin>/backdrop.png`, with the live parts painted out and the selection ring redrawn as a plain border, plus one icon per node. It then rewrites the document's `skins` with the shapes and listeners that draw those parts from the inputs. Rebuild after changing a design or the spec. It needs Pillow and NumPy:

```sh
python3 packages/signal_flow/skins/build.py
```

## Test

```sh
./target/release/osprey test packages/signal_flow/tests
```

The tests start the real document and adapter through the framework's resident `start`/`dispatch` boundary. Every control is reached by its accessible label and pressed with pointer events, in all four skins.
