# Prism synth

A sample application on [@osprey/signal_flow](../../packages/signal_flow/README.md): the signal-flow editor in its Prism skin, wired to a real voice ([UI-MACHINE-FLOW-SYNTH](../../docs/specs/platforms/UiMachines.md)). The graph's five nodes are the synth:

| Node | What it does to the sound |
| --- | --- |
| Oscillator | A 220 Hz sine; bypassed, the synth is silent |
| LFO | Sweeps the filter's cutoff up to two octaves each way, twice a second; Depth sets how far |
| Filter | A two-pole low-pass whose Cutoff slider sweeps 20 Hz to 20 kHz; bypassed, the tone passes untouched |
| Master | The output; bypassed, the synth is silent |
| Scope | Display only |

Every edit that changes the sound renders one second of it in Osprey, as an 8-bit WAV ([src/synth.ospml](src/synth.ospml)), and asks the host to play it through the `run` capability (`perl` writes the clip, then `afplay` plays it). Selecting nodes, the skin menu and undoing to the same patch stay quiet. [src/studio.ospml](src/studio.ospml) is the whole application: the editor's own callbacks, plus the voice.

## Run

```sh
make -C apps/macos synth             # or: bash examples/synth/run.sh
```

## Test

```sh
./target/release/osprey test examples/synth/test
```

The tests check the voice sample by sample: peaks for each patch and the WAV header. Then they drive the Prism canvas by accessible label and check that every audible edit asks the host to play exactly the clip its patch renders, and that every quiet interaction asks for nothing.
