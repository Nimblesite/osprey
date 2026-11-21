/-! Canvas placement, card geometry and musical keyboard [PRISM-ROUTING-LAYOUT]. -/
namespace synth.CanvasGeometry

/-- Node centres leave a dedicated palette at the left of the canvas. -/
def gridX (column : Float) : Float := 350 + column * 310
def gridY (row : Float) : Float := 170 + row * 235
/-- Leave room between the widest Prism cards for the cable and both sockets. -/
def signalX (depth : Float) : Float := 350 + depth * 340
def collides (x y otherX otherY : Float) : Bool :=
  Float.abs (x - otherX) < 320 && Float.abs (y - otherY) < 205
/-- Centre a row of control sockets with a gap between their 28-point hit targets. -/
def controlSocketX (centre rank count : Float) : Float := centre + (rank - (count - 1) / 2) * 34
def offset (pointer centre : Float) : Float := pointer - centre
/-- Right-anchored artwork and its hit areas share the viewport expansion. -/
def shifted (base extra : Float) : Float := base + extra
private def bounded (value low high : Float) : Float :=
  if value < low then low else if value > high then high else value
def panOffset (origin grab pointer last first : Float) : Float :=
  let limit := if last > first then last - first else 0
  bounded (origin + grab - pointer) 0 limit
/-- Node hit areas remain entirely in the graph, outside the palette. -/
def dragX (pointer grab spareWidth : Float) : Float := bounded (pointer - grab) 155 (1065 + spareWidth)
def dragY (pointer grab spareHeight : Float) : Float := bounded (pointer - grab) 155 (700 + spareHeight)
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
/-- The cable painter and hit testing share the same cubic curve. -/
private def cubic (a b c d t : Float) : Float :=
  (1-t)*(1-t)*(1-t)*a + 3*(1-t)*(1-t)*t*b + 3*(1-t)*t*t*c + t*t*t*d
def cableX (source target index : Float) : Float := cubic source (source+70) (target-70) target (index/48)
def cableY (source target index : Float) : Float := cubic source source target target (index/48)
/-- A bounded point-to-segment test avoids gaps between sampled cable points. -/
def nearSegment (px py ax ay endX endY : Float) : Bool :=
  let dx := endX-ax
  let dy := endY-ay
  let length := dx*dx+dy*dy
  let t := if length == 0 then 0 else bounded (((px-ax)*dx+(py-ay)*dy)/length) 0 1
  let ex := px-(ax+t*dx)
  let ey := py-(ay+t*dy)
  ex*ex+ey*ey <= 100
/-- Static source display decimation, independent of live audio activity. -/
def grainIndex (index count : Float) : Float := Float.floor (index * (count - 1) / 47)
def grainX (centre index : Float) : Float := centre - 116 + index * 232 / 47
end synth.CanvasGeometry
