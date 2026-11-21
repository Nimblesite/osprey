/-! Normalised PCM decoding [SYNTH-ADVANCED-GRAINS-IMPORT]. -/
namespace synth.WaveMath
/-- Eight-bit PCM is offset binary; wider PCM is two's complement. -/
def pcm (word bits : Float) : Float :=
  if bits == 8 then (word - 128) / 128
  else
    let half := Float.pow 2 (bits - 1)
    let signed := if word >= half then word - half * 2 else word
    signed / half
/-- Explicit stereo-to-mono downmix, preserving bounded input headroom. -/
def mono (left right : Float) : Float := (left + right) * 0.5
/-- Per-channel sum with independent linear gains at the two mixer inlets. -/
def mixed (first firstGain second secondGain : Float) : Float := first * firstGain + second * secondGain
/-- Source-sample advance per output sample, independent of recording duration. -/
def playbackRate (sourceRate outputRate targetFrequency rootFrequency : Float) : Float :=
  (sourceRate / outputRate) * (targetFrequency / rootFrequency)
/-- Duration of a complete decoded sample at its original sample rate. -/
def seconds (frames rate : Float) : Float := frames / rate
/-- Whole frames at `rate` covering a tail declared in samples at the 48 kHz graph rate. -/
def tailFrames (samples rate : Float) : Float := Float.ceil (samples * rate / 48000)
end synth.WaveMath
