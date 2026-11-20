# daw

A command-line renderer and player for song projects: a JSON piano roll that `@osprey/synth` renders to audio. It can
continue a recording you already have: the engine holds the recording's last moment as a pad, the written
music grows out of it, and the same held sound can return later at other pitches.

```sh
make -C src/toolchain/targets/macos daw-render   # phosphor-drift.json -> target/daw/phosphor-drift/song.wav
make -C src/toolchain/targets/macos daw-play     # plays that WAV; never renders
```

`SONG=` picks another project for either target. The player itself is `daw <project.json> <output-folder>`.
Rendering is offline and slow: the included song takes about 90 minutes at 48 kHz.

## The project file

One JSON object, decoded in [song_files.ospml](src/song_files.ospml) into the typed arrangement in
[daw.td](daw.td).

| Key | Meaning |
| --- | --- |
| `title` | Shown when rendering starts |
| `tempo` | Beats per minute, 20–400 |
| `sampleRate` | Frames per second, 8,000–192,000 |
| `drive` | Master soft limiter, 0.1 (nearly linear) to 4 |
| `space` | The shared reverb: `size` 0.25–2, `decay` in seconds, `damping` 0–0.95 |
| `intro` | The recording the piece continues: `recording` (a path, `~/` for your home folder, otherwise relative to the folder you run from), `tail` seconds the engine holds (0.5–10), `crossfade` seconds into the piece (at most the tail) |
| `sustains` | Appearances of the held tail of the intro: `start` beat, pitch `ratio` (1 as recorded, 1.3333 a fourth up), `rise`, `hold` and `fade` in seconds, `gain`, `reverb` send |
| `tracks` | The piano roll, below |

A track has a `name`, an `instrument`, `gain` (0–4), `pan` (−1 left to 1 right), a `reverb` send (0–1)
and `notes`. A note is `[pitch, start, length, velocity]`: pitch is a MIDI note number, start and length
are in beats, velocity is 0–1.

| `kind` | Sound | Controls |
| --- | --- | --- |
| `pluck` | Waveguide string, damped when the note ends | `brightness` 0–1, `decay` seconds |
| `bow` | Bowed string with delayed vibrato | `pressure` 0–1, `brightness` 0–1, `vibrato` depth 0–0.02 |
| `bell` | Eight bell modes, struck once, ringing for `decay` | `decay` seconds, `brightness` 0–1 |
| `mallet` | Eight bar modes, struck once | `decay` seconds, `brightness` 0–1 |
| `operator` | Two detuned phase-distortion operators | `breakpoint` 0.001–0.999 (0.5 is a sine), `feedback` 0–1, `attack`, `release`, `bloom` (seconds for the tone to mellow), `detune` cents |
| `drum` | Sine body with a pitch drop plus a noise burst | `sweep` 0–8, `noise` 0–1, `colour` 0–1 (0 is white), `decay` up to 1 s |
| `patch` | A routed graph built per note | `nodes`, `cables`, `attack`, `release` (below) |

A `patch` instrument is routing: `nodes` are `{"id", "module", "size", "seed", "set"}` and `cables` are
`{"from", "outlet", "to", "inlet"}`. A value in `set` is a number, `{"pitch": x}` (the note's frequency
times `x`) or `{"velocity": x}`. The modules are `exciter`, `sine`, `wavetable`, `additive`, `operator`,
`bowed`, `lfo`, `resonator`, `modal`, `waveguide`, `strikeComb`, `fractionalDelay`, `lossFilter`,
`allpass`, `lowpass`, `shaper`, `shifter`, `spectral`, `reverb`, `smoother`, `gain`, `modulatedGain`,
`mixer`, `delay` and `output`. An unknown module or setting is an error.

## How it renders [SYNTH-ARRANGEMENT]

A piano roll rendered offline, outside the graph: [daw.td](daw.td) declares the `Arrangement` (tempo, sample rate, master `drive`, reverb `Space`, the `Intro` recording it continues (path, held tail, crossfade), sustain appearances, tracks of `[pitch, start, length, velocity]` notes) and six instruments built from `@osprey/synth` modules. `NoteRenders.render` turns one note into a mono buffer; `Arrangements` renders a track in start order with an overlap-add carry, adds tracks to left, right and reverb buses (`layered`, `added`), and `mixed` runs two feedback-delay reverbs of different size (one per side) and a tanh soft limiter that puts the peak at 0.89. `Sustains.frozen` analyses a recording to its end and holds its last spectrum with the phase-locked freeze; `Sustains.voiced` reads that hold at a speed ratio with its own rise, hold and fade, so written music can continue a recording and bring its last sound back at other pitches (linear interpolation, not bandlimited). Formulas are in [ArrangeMath.lean](src/ArrangeMath.lean).

| Instrument | Built from | Notes |
| --- | --- | --- |
| `PluckInstrument` | `Exciters` noise burst of one period into `Waveguides` | released notes damp to a 0.12 s decay and ring 0.3 s past the gate |
| `BowInstrument` | `BowedStrings`, bow velocity follows the gate (80 ms in, 100 ms out) | 5.4 Hz vibrato fades in; pitch must be below a quarter of the rate |
| `BellInstrument`, `MalletInstrument` | `ModalBanks` with bell or bar ratios, one impulse | ring for `decay` whatever the note length |
| `OperatorInstrument` | two `PhaseOperators` detuned up and down by `detune` cents | the phase-distortion breakpoint eases toward a sine over `bloom` |
| `DrumInstrument` | sine `PhaseOperators` body with a pitch drop plus an `Exciters` noise burst | lasts `decay` (at most 1 s) whatever the note length |
| `PatchInstrument` | `PatchVoices`: a `Graphs` graph built per note from `Routing` nodes and cables, rendered by `Renderer` in 8,192-frame blocks | settings follow the note's pitch or velocity; mono only, so no LFO routing or granular |

Limits: rendering is not realtime and allocates per sample; a note lasts at most 960,000 frames and a piece 28,800,000; tracks are mono before panning; the limiter is not oversampled; there are no automated tests for these modules yet.

## What the host tools do

Osprey writes text files, so each rendered part is written as hexadecimal and `xxd` turns it into a WAV.
`ffmpeg` cuts the intro's last `tail` seconds for the engine to analyse, joins the parts, and crossfades
the piece in over the intro's final `crossfade` seconds. `daw-play` plays the result with `afplay`. Every command runs under
`timeout`. Nothing else touches the audio: every note, the mix, the reverb, the limiter and the held intro
pad come from the engine.

## Limits

- macOS only: playback uses `afplay`. Rendering needs `ffmpeg`, `xxd` and `timeout` on the path.
- There are no automated tests for the player or the arrangement modules yet.
- The limiter is a plain tanh curve without oversampling, and the piece is rendered in mono per track
  before panning, so detuned operators do not spread across the stereo field.
- The intro's tempo and key are not detected. `phosphor-drift.json` was written for the Broken Circuitry recording by
  measuring it first: a pulse of about 97.5 beats per minute and an ending on a C drone in the D♭ major
  scale. The piece is in F Phrygian, so the drone is its dominant, and the held drone returns a fourth up
  and a fifth down (both F) in the breakdown and the ending.
- Routed patches render through the mono reference renderer at about 1.3 seconds per second of sound
  for a simple patch. It rejects `lfo` and `modulatedGain` routing and stereo modules (granular); a
  `bowed` node costs about 20 seconds per second, and `spectral` and `reverb` nodes are too slow to use
  per note. The whole song took about 90 minutes to render at 48 kHz.
- The recording itself is not in the repository; `phosphor-drift.json` expects it at
  `~/Documents/Audio/Broken Circuitry.wav`.
- The two sides use reverbs of different size, so a steady low tone sent to the reverb adds in phase on
  one side and out of phase on the other. At a send of 0.3 the held drone measured 0.47 RMS on the right
  against 0.29 on the left; the song keeps its sustains dry for that reason.
