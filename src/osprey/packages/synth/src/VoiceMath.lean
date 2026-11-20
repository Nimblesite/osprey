/-! Reference voice and state-variable filter mathematics [SYNTH-VOICE] [SYNTH-DSP].
Filter reference: Zavalishin, "The Art of VA Filter Design", trapezoidal SVF. -/
namespace synth.VoiceMath

def pi : Float := 3.141592653589793
def tau : Float := 6.283185307179586

/-- Prewarped integrator gain `tan (π fc / fs)`. -/
def warped (cutoff sampleRate : Float) : Float :=
  Float.sin (pi * cutoff / sampleRate) / Float.cos (pi * cutoff / sampleRate)

/-- Damping `k = 1 / Q`. -/
def damping (resonance : Float) : Float := 1 / resonance

def gainOne (g k : Float) : Float := 1 / (1 + g * (g + k))
def gainTwo (g k : Float) : Float := g * gainOne g k
def gainThree (g k : Float) : Float := g * g * gainOne g k

def bandpass (a1 a2 one two input : Float) : Float := a1 * one + a2 * (input - two)
def lowpass (a2 a3 one two input : Float) : Float := two + a2 * one + a3 * (input - two)
def highpass (k input b l : Float) : Float := input - k * b - l
/-- Trapezoidal integrator update `2 v − s`. -/
def integrated (out held : Float) : Float := 2 * out - held

/-- MIDI key number from octave, semitone and sharp offset. -/
def midiKey (octave semitone sharp : Float) : Float := 12 * (octave + 1) + semitone + sharp

/-- LFO-swept cutoff at sample `n`. -/
def swept (cutoff depth octaves lfo n rate : Float) : Float :=
  cutoff * Float.pow 2 (depth * octaves * Float.sin (tau * lfo * n / rate))
/-- Two-pole Butterworth low-pass magnitude at `pitch`. -/
def butterworth (pitch cutoff : Float) : Float := 1 / Float.sqrt (1 + Float.pow (pitch / cutoff) 4)

/-- Naive (aliasing) triangle and saw, as the reference voice draws them; `Waves` has the band-limited ones. -/
def triangle (t : Float) : Float := 1 - 4 * Float.abs (t - 0.5)
def saw (t : Float) : Float := 2 * t - 1

/-- A 0–1 cutoff knob swept exponentially from 20 Hz to 20 kHz. -/
def knobCutoff (amount : Float) : Float := 20 * Float.pow 1000 amount

end synth.VoiceMath
