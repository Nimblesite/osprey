# Songs

A command-line player for song projects: a JSON piano roll that `@osprey/synth` renders to audio. It can
continue a recording you already have: the engine holds the recording's last moment as a pad, the written
music grows out of it, and the same held sound can return later at other pitches.

```sh
make -C src/toolchain/targets/macos songs INTRO="/path/to/Broken Circuitry.wav"
```

That builds the player, renders `broken-circuitry.json`, writes `target/songs/out/song.wav` and plays it.
`SONG=` picks another project, and leaving `INTRO` out renders the piece on its own. The player itself is

```sh
songs <project.json> <output-folder> [intro-recording] [--silent]
```

`--silent` writes the file without playing it. Rendering is offline and slow: the included song takes
several minutes.

## The project file

One JSON object, decoded in [song_files.ospml](src/song_files.ospml) into the typed arrangement in
[arrangement.td](../../packages/synth/arrangement.td).

| Key | Meaning |
| --- | --- |
| `title` | Shown when rendering starts |
| `tempo` | Beats per minute, 20–400 |
| `sampleRate` | Frames per second, 8,000–192,000 |
| `drive` | Master soft limiter, 0.1 (nearly linear) to 4 |
| `space` | The shared reverb: `size` 0.25–2, `decay` in seconds, `damping` 0–0.95 |
| `sustains` | Appearances of the held end of the intro: `start` beat, pitch `ratio` (1 as recorded, 1.3333 a fourth up), `rise`, `hold` and `fade` in seconds, `gain`, `reverb` send |
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

## What the host tools do

Osprey writes text files, so each rendered part is written as hexadecimal and `xxd` turns it into a WAV.
`ffmpeg` cuts the last 1.5 seconds of the intro for the engine to analyse, joins the parts, and crossfades
the piece in over the intro's final 0.35 seconds. `afplay` plays the result. Every command runs under
`timeout`. Nothing else touches the audio: every note, the mix, the reverb, the limiter and the held intro
pad come from the engine.

## Limits

- macOS only: playback uses `afplay`. Rendering needs `ffmpeg`, `xxd` and `timeout` on the path.
- There are no automated tests for the player or the arrangement modules yet.
- The limiter is a plain tanh curve without oversampling, and the piece is rendered in mono per track
  before panning, so detuned operators do not spread across the stereo field.
- The intro's tempo and key are not detected. `broken-circuitry.json` was written for that recording by
  measuring it first: a pulse of about 97.5 beats per minute and an ending on a C drone in the D♭ major
  scale. The piece is in F Phrygian, so the drone is its dominant, and the held drone returns a fourth up
  and a fifth down (both F) in the breakdown and the ending.
- The recording itself is not in the repository; without `INTRO` the sustains are skipped.
- The two sides use reverbs of different size, so a steady low tone sent to the reverb adds in phase on
  one side and out of phase on the other. At a send of 0.3 the held drone measured 0.47 RMS on the right
  against 0.29 on the left; the song keeps its sustains dry for that reason.
