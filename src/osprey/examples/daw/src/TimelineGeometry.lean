/-! Timeline, piano roll and mix table placement, in Prism's 1536 × 828 scene units [DAW-PAGES]. -/
namespace synth.TimelineGeometry

private def bounded (value low high : Float) : Float :=
  if value < low then low else if value > high then high else value

/-- The bar (from 0, four beats each) a beat falls in. -/
def bar (beat : Float) : Float := Float.floor (beat / 4)
/-- The beat a bar (from 0) starts on. -/
def barStart (index : Float) : Float := index * 4

/-- Arrangement lanes span x 230 to 1200 across the whole song. -/
def laneX (beat beats : Float) : Float := 230 + beat / beats * 970
def laneBeat (x beats : Float) : Float := bounded ((x - 230) / 970 * beats) 0 beats

/-- Track rows start at y 146 and share 330 units, at most 22 each. -/
def rowHeight (count : Float) : Float := if count <= 15 then 22 else 330 / count
def rowY (index height : Float) : Float := 146 + index * height
def rowAt (y height : Float) : Float := Float.floor ((y - 146) / height)

/-- The piano roll shows two bars (eight beats) from x 70 to 1200. -/
def rollX (beat first : Float) : Float := 70 + bounded (beat - first) 0 8 * 141.25
def rollWidth (beat length first : Float) : Float :=
  rollX (beat + length) first - rollX beat first
/-- Pitch rows fill y 520 to 750, the highest pitch at the top. -/
def pitchHeight (rows : Float) : Float := 230 / rows
def pitchY (pitch high rows : Float) : Float := 520 + (high - pitch) * (230 / rows)

/-- Velocity stems share the same note start coordinate as the piano roll. -/
def velocityHeight (value : Float) : Float := bounded value 0 1 * 43
/-- Mix table rows start at y 150 and share 640 units, at most 36 each. -/
def mixHeight (count : Float) : Float := if count <= 17 then 36 else 640 / count
def mixY (index height : Float) : Float := 150 + index * height
def mixAt (y height : Float) : Float := Float.floor ((y - 150) / height)

/-- A level meter's filled width: `value` of `full`, within `width`. -/
def meter (value full width : Float) : Float := bounded (value / full) 0 1 * width
/-- Read a horizontal fader on its labelled range, rounded to hundredths. -/
def fader (x left width low high : Float) : Float :=
  Float.floor ((low + bounded ((x - left) / width) 0 1 * (high - low)) * 100 + 0.5) / 100
/-- Whole percent and hundredths, rounded to nearest. -/
def percent (value : Float) : Float := Float.floor (value * 100 + 0.5)
