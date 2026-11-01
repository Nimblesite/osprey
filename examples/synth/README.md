# Prism synth

A sample application on [@osprey/signal_flow](../../packages/signal_flow/README.md): the signal-flow editor in its Prism skin with a keyboard deck, wired to a real voice ([UI-MACHINE-FLOW-SYNTH](../../docs/specs/platforms/UiMachines.md)). The graph's five nodes are the synth:

| Node | What it does to the sound |
| --- | --- |
| Oscillator | A sine, triangle, saw or square at the struck key's pitch; bypassed, the synth is silent |
| LFO | Sweeps the filter's cutoff up to two octaves each way, twice a second; Depth sets how far |
| Filter | A two-pole low-pass whose Cutoff slider sweeps 20 Hz to 20 kHz; bypassed, the tone passes untouched |
| Master | The output; bypassed, the synth is silent |
| Scope | Display only |

Click a key on the keyboard under the graph to play: it renders one second of the patch at that key's pitch in Osprey, as an 8-bit WAV ([src/synth.ospml](src/synth.ospml)), and asks the host to play it through the `run` capability (`perl` writes the clip, then `afplay` plays it). The controls shape the sound and never play it. [src/studio.ospml](src/studio.ospml) is the whole application: the editor's own callbacks, plus the keyboard and the voice.

## Run

```sh
make -C apps/macos synth             # or: bash examples/synth/run.sh
```

## Test

```sh
./target/release/osprey test examples/synth/test
```

The tests check the voice sample by sample: peaks for each patch and the WAV header. Then they drive the Prism canvas by accessible label and check that every key asks the host to play exactly the clip its patch renders at its pitch, and that every control asks for nothing.
