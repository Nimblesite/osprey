/-! Canvas placement, card geometry and musical keyboard [PRISM-ROUTING-LAYOUT]. -/
namespace synth.CanvasGeometry

/-- Node centres leave a dedicated palette at the left of the canvas. -/
def gridX (column : Float) : Float := 350 + column * 310
def gridY (row : Float) : Float := 170 + row * 235
def offset (pointer centre : Float) : Float := pointer - centre
private def bounded (value low high : Float) : Float :=
  if value < low then low else if value > high then high else value
/-- Node hit areas remain entirely in the graph, outside the palette. -/
def dragX (pointer grab : Float) : Float := bounded (pointer - grab) 155 1065
def dragY (pointer grab : Float) : Float := bounded (pointer - grab) 100 700
def valid (px py : Float) : Bool :=
  px == px && py == py && Float.abs px <= 1000000 && Float.abs py <= 1000000
/-- Equal temperament, A4 = 440 Hz. -/
def frequency (note : Float) : Float := 440 * Float.pow 2 ((note - 69) / 12)
/-- Palette row centre and keyboard white-key left edge. -/
def paletteY (index : Float) : Float := 130 + index * 48
def keyX (index : Float) : Float := 24 + index * 56
/-- Miniature waveform coordinates are bounded by the card's display. -/
def waveX (centre index : Float) : Float := centre - 94 + index * 3
def waveY (centre sample : Float) : Float := centre - bounded sample (-1) 1 * 25
/-- Draw the oscillator's configured sine shape, not an invented audio meter. -/
def sinePoint (index : Float) : Float := Float.sin (index * 0.3)
/-- Match the original Prism logarithmic cutoff slider. -/
def cutoffPosition (hz : Float) : Float := Float.log (hz / 20) / Float.log 1000
/-- Inverse of the original logarithmic cutoff slider. -/
def cutoffFrequency (position : Float) : Float := bounded (20 * Float.pow 1000 (bounded position 0 1)) 20 19000
end synth.CanvasGeometry
