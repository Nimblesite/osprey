# daw

A digital audio workstation on the FerroUI host: one song project, shown as a timeline, a mix and a
patch per track. Every track's sound is a patch you edit in Prism, and everything (notes, mix and patches)
is saved in the one project file. [Specification](../../../../docs/specs/applications/Daw.md) ·
[Designs](../../../../docs/designs/prism-timeline/README.md) · [Patch editor guide](PRISM.md)

```sh
make -C src/toolchain/targets/macos prism          # installs /Applications/Prism.app for Spotlight
make -C src/toolchain/targets/macos daw-render     # renders the whole SONG to target/daw/<project>/song.wav
make -C src/toolchain/targets/macos daw-play       # plays that WAV; never renders
make -C src/toolchain/targets/tests synth          # the app's tests, with no host
```

| Page | What it does |
| --- | --- |
| ![Timeline](screens/timeline.png) | **Timeline.** One lane per track across the song, and a two-bar piano roll of the selected track. Tap a lane to select the track at that bar. Use the rail at the right of the lanes to reach the next seven tracks. Tap a track name or the Patch tab to edit its sound. View → Keyboard opens the playable keys. |
| ![A track's patch](screens/track-patch.png) | **Patch.** The selected track's patch in Prism: modules, cables and the selected module’s connections. Leaving the page saves the patch into the project file. A built-in instrument opens as one voice module. |
| ![Mix](screens/mix.png) | **Mix.** The selected track’s dry path passes through its assigned bus or goes directly to Master. The group card shows its inputs and compressor detector. Click its output or use the **Routing** menu to choose Master, assign a project bus or create a named stereo group. |

File → Save writes the open song, its patches and their layouts. Play renders two bars from the playhead. Stop cancels playback. File → Open Song opens another project in the same window.

## The songs are hand-written

Every song in [tracks/](tracks/) is composed by hand: each note, pattern, patch and mix setting is written into its project file one at a time. No script, loop or generator ever writes notes, patterns, arrangements or project files; a song made that way is not accepted. Each song has its own style, its own synthesis techniques and its own arrangement.

## Silt Relay

[Silt Relay](tracks/silt-relay.json) is an original 18-bar arrangement at 138 BPM. The pressure kick ducks the sub. Struck wire answers the bass on the offbeats, and the bowed breakdown gives way to a five-hit hi-hat roll. Open **Throat / metal** for a two-source spectral morph, **Spore wheel** for a seeded granular patch, or **Rivet pulse** to follow a MIDI note through the split, delay, join and channel swap. On **Glass pins**, the FFT pitch shifter mixes a perfect fifth into the metallic strike at 22% wet and keeps the original formants. Glass pins, Glass filament and Porcelain bell share a gentle high lift and compression in Pin bed before they meet the struck wire on Objects. **Rivet pulse** sends 6% of its stereo channel-swap outlet straight to the kick-keyed Undertow bus. Its dry signal still goes through Objects, and its 19% Space send stays audible between hits. [Follow Glass pins through both buses](screens/silt-pin-bed.png) · [Inspect Undertow](screens/silt-relay.png) · [Trace the three Rivet pulse routes](screens/silt-rivet.png)

```sh
make -C src/toolchain/targets/macos prism SONG=src/osprey/examples/daw/tracks/silt-relay.json
```

In **Bus routing**, choose **Routing → Bus outputs…** to feed a group into another group. Pick the source and stereo output to see its whole path to Master. A cycle displays the names of the groups involved and keeps the last saved route. [Group outputs](screens/bus-outputs.png)

## The project file

The app and renderer use one JSON project. The field readers and writer are generated from [daw.td](daw.td). Project validation and error messages live in [song_files.ospml](src/song_files.ospml).

| Key | Meaning |
| --- | --- |
| `title` | The song's name |
| `tempo` | Beats per minute, 20–400 |
| `sampleRate` | Frames per second, 8,000–192,000 |
| `buses` | Named stereo groups with gain, three-band EQ, a compressor and an `output` group. An empty output feeds Master |
| `tracks` | The tracks, below |

A track has a `name`, an `instrument`, `gain` (0–4), `pan` (−1 left to 1 right),
`bus` (the dry destination), a list of internal outlet `sends`,
`positions` (where its patch's modules sit in Prism) and `notes`. A note is
`{"pitch", "start", "length", "velocity"}`: pitch is a MIDI note number, start and length are in beats,
velocity is 0–1. An instrument's `kind` names its type in daw.td:

| `kind` | Sound | Controls |
| --- | --- | --- |
| `PluckInstrument` | Waveguide string, damped when the note ends | `brightness` 0–1, `decay` seconds |
| `BowInstrument` | Bowed string with delayed vibrato | `pressure` 0–1, `brightness` 0–1, `vibrato` depth 0–0.02 |
| `BellInstrument` | Eight bell modes, struck once, ringing for `decay` | `decay` seconds, `brightness` 0–1 |
| `MalletInstrument` | Eight bar modes, struck once | `decay` seconds, `brightness` 0–1 |
| `OperatorInstrument` | Two detuned phase-distortion operators | `tone`: `breakpoint` 0.001–0.999 (0.5 is a sine), `feedback` 0–1, `attack`, `release`, `bloom` (seconds for the tone to mellow), `detune` cents |
| `DrumInstrument` | Sine body with a pitch drop plus a noise burst | `hit`: `sweep` 0–8, `noise` 0–1, `colour` 0–1 (0 is white), `decay` up to 1 s |
| `PatchInstrument` | A routed graph built per note: what Prism edits | `routing`: `nodes`, `cables`, `attack`, `release` (below) |

A routing's `nodes` are `{"identity", "component", "size", "seed", "asset", "settings"}` and its `cables` are
`{"source", "outlet", "destination", "inlet"}`. A setting is `{"key", "value", "follow"}`: `follow` is
`Constant`, `Pitched` (the note's frequency times `value`) or `Struck` (the note's velocity times `value`).
The available graph components include `exciter`, `sine`, `wavetable`, `additive`, `operator`,
`bowed`, `lfo`, `resonator`, `modal`, `waveguide`, `strikeComb`, `fractionalDelay`, `lossFilter`,
`allpass`, `lowpass`, `shaper`, `shifter`, `spectral`, `reverb`, `smoother`, `gain`, `modulatedGain`,
`mixer`, `delay`, `output`, `modulatedLowpass`, `grainCloud`, `midiNote`, `formulaNotes` (its formula in `asset`), `midiOscillator`, `stereo`, `downmix`, `split`, `join`, `route`, `stereoModulatedGain` and `stereoOutput`. Each `channelN` setting on a `route` selects one input for output N. An unknown component or setting is an error.

### Send a patch outlet to a bus

On Track patch or Bus routing, choose **Routing → Add send…**. Select a source outlet and destination bus, enter a percentage, and choose **Connect send**. The inspector lists the other cables and sends leaving that outlet. Choose **Routing → Patch sends…** to see each tap’s source, level, mute state and path to Master. Click **Edit send** to change a tap or remove it. Create a destination with **New stereo bus…** in Bus routing.

For example, `{"sendId":1,"source":3,"outlet":0,"bus":"pads","level":0.25,"enabled":true}` sends a quarter of node 3’s first audio outlet to pads. The existing dry cable keeps playing. The project file holds the whole edit.

## How it renders [SYNTH-ARRANGEMENT]

**Playing and rendering both take Master only, and they are different things.** `Playback` runs the project graph forward one block at a time: every track, send, bus, the Space and Master advance together, and only Master's frames leave the block ([DAW-RENDER-PLAY]). **Playing** (`daw … live|loop`) streams those Master blocks to the audio device and writes no file. **Rendering** (`daw <project.json> <folder>`, `Player`) writes Master's output, untouched, to the export file `song.wav` ([DAW-RENDER-MASTER]). Exporting anything other than Master is banned. No note, track or bus is rendered on its own, ahead of time, into a cache or as a stem. Code that still makes notes outside the block graph breaks this rule and is listed as open in the [plan](../../../../docs/plans/daw.md). [daw.td](daw.td) declares the `Arrangement` (title, tempo, sample rate, buses, tracks of `[pitch, start, length, velocity]` notes) and six instruments built from `@osprey/synth` modules. `Sustains.frozen` analyses a recording to its end and holds its last spectrum with the phase-locked freeze. `Sustains.voiced` reads that hold at a speed ratio with its own rise, hold and fade, so written music can continue a recording and bring its last sound back at other pitches (linear interpolation, not bandlimited). Formulas are in [arrange_math.ospml](src/arrange_math.ospml).

| Instrument | Built from | Notes |
| --- | --- | --- |
| `PluckInstrument` | `Exciters` noise burst of one period into `Waveguides` | released notes damp to a 0.12 s decay and ring 0.3 s past the gate |
| `BowInstrument` | `BowedStrings`, bow velocity follows the gate (80 ms in, 100 ms out) | 5.4 Hz vibrato fades in; pitch must be below a quarter of the rate |
| `BellInstrument`, `MalletInstrument` | `ModalBanks` with bell or bar ratios, one impulse | ring for `decay` whatever the note length |
| `OperatorInstrument` | two `PhaseOperators` detuned up and down by `detune` cents | the phase-distortion breakpoint eases toward a sine over `bloom` |
| `DrumInstrument` | sine `PhaseOperators` body with a pitch drop plus an `Exciters` noise burst | lasts `decay` (at most 1 s) whatever the note length |
| `PatchInstrument` | `PatchVoices` builds a graph per note. The scheduler uses 8,192-frame blocks. Graphs with MIDI, control signals or multichannel ports run through `GraphClips` in blocks of up to 1,024 frames at 48 kHz | settings can follow note pitch or velocity |

Limits: rendering is not realtime and allocates per sample; a note lasts at most 960,000 frames and a piece 28,800,000. Every track carries the project's channel layout, stereo by default ([DAW-MODULAR-LAYOUT]): a patch's stereo output keeps both channels, and a mono output or built-in voice sits in the centre.

## What the host tools do

Osprey writes text files, so each rendered part is written as hexadecimal and `xxd` turns it into a WAV.
`ffmpeg` joins the parts. Every command runs under `timeout`. Nothing else touches the audio: every sound comes from the graph.
No processing is hidden. There is no built-in limiter or clip, and everything that changes the signal is visible in the UI ([DAW-TRANSPARENCY]).

## Limits

- macOS only: playback uses `afplay`. Rendering needs `ffmpeg`, `xxd` and `timeout` on the path.
- Spectral processing and patch reverb can take minutes to render offline. *Silt Relay* combines them on **Throat / metal** and uses granular synthesis and MIDI routing on two other tracks. The 34.49-second stereo master with the pitch-shifted Glass pins and parallel Rivet pulse send took about fourteen minutes to render on the development Mac.
