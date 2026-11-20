/-! Piano-roll timing, note envelopes and mix gains [SYNTH-ARRANGEMENT]. -/
namespace daw.ArrangeMath

/-- Frames covered by `beats` at `tempo` beats per minute. -/
def beatFrames (beats tempo sampleRate : Float) : Float := Float.floor (beats * 60 / tempo * sampleRate + 0.5)

/-- Frames covered by `time` seconds. -/
def secondFrames (time sampleRate : Float) : Float := Float.floor (time * sampleRate + 0.5)

/-- Gate envelope: linear attack over `attack` frames, hold while the note is down, linear release over
`release` frames after `gate`; exactly zero once released. -/
def gated (frame gate attack release : Float) : Float :=
  let rise := if frame >= attack then 1 else frame / attack
  if frame < gate then rise
  else if frame >= gate + release then 0
  else rise * (1 - (frame - gate) / release)

/-- Exponential ring reaching −60 dB after `decay` seconds. -/
def ringing (frame decay sampleRate : Float) : Float :=
  Float.exp (-6.907755278982137 * frame / (decay * sampleRate))

/-- Drum pitch sweep: starts `sweep` times above the note and settles with a 25 ms time constant. -/
def swept (frequency sweep frame sampleRate : Float) : Float :=
  frequency * (1 + sweep * Float.exp (-frame / (0.025 * sampleRate)))

/-- Phase-distortion breakpoint easing from its struck value toward 0.5 (a sine) over `bloom` seconds. -/
def bloomed (breakpoint frame bloom sampleRate : Float) : Float :=
  0.5 - (0.5 - breakpoint) * Float.exp (-frame / (bloom * sampleRate))

/-- Delayed 5.4 Hz vibrato: `depth` is the peak relative deviation, reached about 1 s into the note. -/
def vibrato (frequency frame depth sampleRate : Float) : Float :=
  let onset := 1 - Float.exp (-frame / (0.35 * sampleRate))
  frequency * (1 + depth * onset * Float.sin (33.929200658769766 * frame / sampleRate))

/-- Frequency moved by `cents` hundredths of a semitone. -/
def detuned (frequency cents : Float) : Float := frequency * Float.pow 2 (cents / 1200)

/-- Equal-power pan gains for pan −1 (left) to 1 (right). -/
def panLeft (pan : Float) : Float := Float.cos (0.7853981633974483 * (pan + 1))
def panRight (pan : Float) : Float := Float.sin (0.7853981633974483 * (pan + 1))

/-- Hyperbolic tangent. -/
def squashed (x : Float) : Float :=
  let doubled := Float.exp (2 * x)
  (doubled - 1) / (doubled + 1)

/-- Master soft limiter: the mix is scaled so its peak is 1, driven into a tanh curve and rescaled so the
peak lands on `ceiling`. A small drive is nearly linear; a larger one raises the average level. -/
def pressed (sample peak drive ceiling : Float) : Float :=
  if peak <= 0 then 0 else ceiling * squashed (drive * sample / peak) / squashed drive

/-- Read position in a held recording played `ratio` times faster, after `skip` frames. -/
def readAt (frame ratio skip : Float) : Float := skip + frame * ratio

/-- A deterministic, normalised single-cycle harmonic sample for the initial grain-cloud patch. -/
def grainSample (index : Float) : Float :=
  0.5 * Float.sin (6.283185307179586 * index / 1024) + 0.25 * Float.sin (18.84955592153876 * index / 1024) + 0.125 * Float.sin (43.982297150257104 * index / 1024)
/-- A track's mix is playable: gain 0–4, pan −1 (left) to 1 (right), Space send 0–1. -/
def mixable (gain pan reverb : Float) : Bool :=
  gain >= 0 && gain <= 4 && pan >= -1 && pan <= 1 && reverb >= 0 && reverb <= 1

/-- A note velocity is 0 (silent) to 1 (full). -/
def audible (velocity : Float) : Bool := velocity >= 0 && velocity <= 1

/-- A note starting at `start` falls in the `beats` long window beginning at `first`. -/
def inWindow (start first beats : Float) : Bool := start >= first && start - first < beats

/-- A start beat moved so the window beginning at `first` starts at beat 0. -/
def shifted (start first : Float) : Float := start - first
/-- `value` times `factor`: a gain applied, or a setting following the note's frequency or velocity. -/
def scaled (value factor : Float) : Float := value * factor

/-- A bus sample with `sample` mixed in at `gain`. -/
def bused (existing gain sample : Float) : Float := existing + gain * sample

/-- `sample` under a gate envelope whose attack and release are in seconds. -/
def enveloped (sample frame gate attack release sampleRate : Float) : Float :=
  sample * gated frame gate (attack * sampleRate) (release * sampleRate)

/-- A held sound's level: rising over `rise` s, holding `hold` s, then fading over `fade` s. -/
def glowLevel (frame rise hold fade sampleRate : Float) : Float :=
  gated frame (secondFrames (rise + hold) sampleRate) (rise * sampleRate) (fade * sampleRate)

/-- Seconds of held sound read: the longest appearance plus the skipped start and two frames of slack. -/
def neededSeconds (longest skipped sampleRate : Float) : Float := longest + (skipped + 2) / sampleRate

/-- Whether `frames` of held sound cover `needed` seconds, within one frame of rounding. -/
def holds (frames needed sampleRate : Float) : Bool := frames + 1 >= needed * sampleRate

/-- Loop storage for one period of `frequency`, with eight frames of headroom. -/
def loopRoom (sampleRate frequency : Float) : Float := Float.ceil (sampleRate / frequency) + 8

/-- A pluck's noise colour: softer notes are darker. -/
def pickColour (velocity : Float) : Float := 0.7 * (1 - velocity)

/-- Bow speed: 0.3 of the velocity, with an 80 ms attack and a 100 ms release. -/
def bowSpeed (velocity frame gate sampleRate : Float) : Float :=
  0.3 * velocity * gated frame gate (0.08 * sampleRate) (0.1 * sampleRate)

/-- One of the operator pair's level: half the velocity under the tone's envelope. -/
def operatorLevel (velocity frame gate attack release sampleRate : Float) : Float :=
  0.5 * velocity * gated frame gate (attack * sampleRate) (release * sampleRate)

/-- A drum body's level: the share of the hit that is not noise, ringing out over `decay` s. -/
def drumBody (velocity noise frame decay sampleRate : Float) : Float :=
  velocity * (1 - noise) * ringing frame decay sampleRate

/-- The piece's frames: the last note end plus the reverb decay and two seconds. -/
def pieceFrames (beats tempo sampleRate decay : Float) : Float := beatFrames beats tempo sampleRate + secondFrames (decay + 2) sampleRate

end daw.ArrangeMath
