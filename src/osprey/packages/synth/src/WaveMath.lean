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
/-- Source-sample advance per output sample, independent of recording duration. -/
def playbackRate (sourceRate outputRate targetFrequency rootFrequency : Float) : Float :=
  (sourceRate / outputRate) * (targetFrequency / rootFrequency)
end synth.WaveMath
