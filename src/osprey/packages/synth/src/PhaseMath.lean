/-! Phase-shaping oscillator mathematics [SYNTH-ADVANCED-PHASE].

Wave Pulse Phase Modulation is taken verbatim from M. Smart, "Wave Pulse Phase Modulation:
Hybridising Phase Modulation and Phase Distortion", DAFx25, Ancona, pp. 166–169, equations
(1) and (3)–(9). Phase is the normalised ramp `φ ∈ [0, 1)` of equation (4).
-/
namespace synth.PhaseMath

/-- Equation (1): the inverted-cosine carrier driven by a shaped phase. -/
def carrier (shaped : Float) : Float := -Float.cos (6.283185307179586 * shaped)

/-- Equation (5): the sinusoidal window `w(t) = sin(π t)`. -/
def window (t : Float) : Float := Float.sin (3.141592653589793 * t)

/-- Equation (6): sine-windowed modified phase modulation. -/
def windowedPhase (phase index ratio : Float) : Float :=
  phase + window phase * index * Float.sin (6.283185307179586 * ratio * phase)

/-- External phase modulation plus DX-style averaged self-feedback, in cycles. -/
def modulated (shaped modulation depth previous older feedback : Float) : Float :=
  shaped + depth * modulation + feedback * (previous + older) / 2

/-- Instantaneous frequency with linear through-zero FM; negative values run the phase backwards. -/
def instantaneous (frequency modulation spread : Float) : Float := frequency + spread * modulation

end synth.PhaseMath
