/-! Timeline, piano roll and mix table placement, in Prism's 1440 × 940 scene units [DAW-PAGES]. -/
namespace synth.TimelineGeometry

private def bounded (value low high : Float) : Float :=
  if value < low then low else if value > high then high else value

/-- The bar (from 0, four beats each) a beat falls in. -/
def bar (beat : Float) : Float := Float.floor (beat / 4)
/-- The bar a beat falls in, but never past the bar holding the song's last sound: a song `beats` long that ends
on a bar line has no bar after it to play [DAW-TRANSPORT-PLAYHEAD]. -/
def playableBar (beat beats : Float) : Float :=
  let last := if beats <= 0 then 0 else Float.ceil (beats / 4) - 1
  bounded (bar beat) 0 last
/-- The beat a bar (from 0) starts on. -/
def barStart (index : Float) : Float := index * 4

/-- The song overview in the strip spans x 468 to 792: one column per bar, two units apart, as tall as the bar is
busy (2 to 25 units above y 122) [DAW-HEADER]. -/
def overviewX (index bars : Float) : Float := 468 + index / bars * 324
def overviewWidth (bars : Float) : Float := 324 / bars - 2
def overviewTop (count most : Float) : Float := if most <= 0 then 120 else 120 - count / most * 23

/-- A song page is 1440 by 940 units and fits its region whole. In a region wider than that shape the page keeps
its height (the region is 940 units tall) and sits in the middle: this is how far right it starts. -/
def centred (viewWidth viewHeight : Float) : Float :=
  if viewHeight > 939 && viewHeight < 941 && viewWidth > 1440 then (viewWidth - 1440) / 2 else 0

/-- Arrangement lanes span x 198 to 1092. They show `span` beats from beat `first`: the whole song until it is
zoomed [DAW-TIMELINE-ZOOM]. -/
def laneX (beat first span : Float) : Float := 198 + (beat - first) / span * 894
def laneBeat (x first span beats : Float) : Float := bounded (first + (x - 198) / 894 * span) 0 beats
/-- A lane position kept between the lanes' two edges. -/
def laneEdge (x : Float) : Float := bounded x 198 1092
/-- Zoom 1 fits the whole song; the closest zoom shows one beat across the lanes. -/
def zoomed (zoom factor beats : Float) : Float :=
  let most := if beats < 1 then 1 else beats
  bounded (zoom * factor) 1 most
def laneSpan (beats zoom : Float) : Float := beats / zoom
/-- The first beat shown: never before the song, nor so late that the view runs past its end. -/
def laneFirst (start span beats : Float) : Float := bounded start 0 (beats - span)
/-- The first beat shown after a zoom to `next` beats that keeps `anchor` where it was; an anchor outside the
view comes to its nearer edge. -/
def zoomFirst (anchor first span next beats : Float) : Float :=
  laneFirst (anchor - bounded ((anchor - first) / span) 0 1 * next) next beats
/-- The first beat shown once the playhead moved: unchanged while the playhead and the two bars after it are in
view, else the playhead comes to the nearer edge. -/
def followFirst (playhead first span beats : Float) : Float :=
  let reach := if span < 8 then span else 8
  if playhead < first then laneFirst playhead span beats
  else if playhead + reach > first + span then laneFirst (playhead + reach - span) span beats
  else first
/-- Ruler divisions in beats: a line every `rulerStep`, a number every `rulerMajor`. Bars with every fourth
numbered, then every bar, then beats, then sixteenths, so labels never crowd or thin out. -/
def rulerStep (span : Float) : Float := if span > 16 then 4 else if span > 4 then 1 else 0.25
def rulerMajor (span : Float) : Float := if span > 64 then 16 else if span > 4 then 4 else 1
def rulerBeat (first step index : Float) : Float := (Float.floor (first / step) + index) * step
def rulerCount (span step : Float) : Float := Float.ceil (span / step) + 2
/-- 1 when `beat` falls on a multiple of `major`, else 0. -/
def onDivision (beat major : Float) : Float :=
  if beat - Float.floor (beat / major) * major < 0.001 then 1 else 0
/-- The beat within its bar, from 1. -/
def beatInBar (beat : Float) : Float := Float.floor (beat - Float.floor (beat / 4) * 4) + 1

/-- The arrangement shows seven 31-unit rows above the roll. -/
/-- [DAW-LAYOUT-FILL] The lanes' scrollbar runs y 221 to 438 beside `rows` visible lanes of `count` tracks. Its thumb
starts at the share of the tracks above the first visible one and is as tall as the share that is visible. -/
def thumbTop (top count : Float) : Float := 221 + 217 * top / count
def thumbHeight (rows count : Float) : Float := 217 * rows / count
/-- The first visible track when the thumb's middle is put under `y`: any row, not only whole pages. -/
def scrollTop (y rows count : Float) : Float :=
  bounded (Float.floor ((y - 221) / 217 * count - rows / 2 + 0.5)) 0 (count - rows)
/-- The first visible track that keeps track `index` in view, moving no further than it must. -/
def keptTop (top index rows count : Float) : Float :=
  let wanted := if index < top then index else if index >= top + rows then index - rows + 1 else top
  let last := if count > rows then count - rows else 0
  bounded wanted 0 last
def rowY (index height : Float) : Float := 221 + index * height
def rowAt (y height : Float) : Float := Float.floor ((y - 221) / height)

/-- The pitch silhouette of a track fits between the two inset edges of its lane. -/
def lanePitch (top height pitch low high : Float) : Float :=
  if high <= low then top + height / 2 else top + 2 + (high - pitch) / (high - low) * (height - 4)

/-- The piano roll shows two bars (eight beats) from x 76 to 1110. -/
def rollX (beat first : Float) : Float := 76 + bounded (beat - first) 0 8 * 129.25
def rollWidth (beat length first : Float) : Float :=
  rollX (beat + length) first - rollX beat first
/-- Pitch rows fill y 542 to 802, the highest pitch at the top. -/
def pitchHeight (rows : Float) : Float := 260 / rows
def pitchY (pitch high rows : Float) : Float := 542 + (high - pitch) * (260 / rows)
/-- [DAW-ROLL-KEYS] The pitch of the key row under `y`: the inverse of `pitchY`. -/
def keyPitch (y high rows : Float) : Float := high - Float.floor ((y - 542) / (260 / rows))
/-- Reserve the native typeface's ascent and descent within the two inset note edges. -/
def noteLabelSize (height : Float) : Float := bounded ((height - 2) / 1.32) 0 12
def noteLabelY (top height size : Float) : Float := top + (height - size * 1.32) / 2 - 1

/-- Velocity stems share the same note start coordinate as the piano roll. -/
def velocityHeight (value : Float) : Float := bounded value 0 1 * 43
/-- Dragging preserves the note's original timing offset and moves by sixteenths (one quarter beat). -/
def draggedStart (start offset : Float) : Float :=
  let beat := start + Float.floor (offset / 129.25 * 4 + 0.5) / 4
  if beat < 0 then 0 else beat
/-- One vertical pitch row is one semitone. -/
def draggedPitch (pitch offset height : Float) : Float :=
  bounded (pitch + Float.floor (offset / height + 0.5)) 0 127
/-- The stem baseline is y 894 and its 43-unit range is the full velocity. -/
def stemVelocity (y : Float) : Float := bounded ((894 - y) / 43) 0 1
/-- Mix table rows start at y 235 and share 629 units, at most 37 each. -/
def mixHeight (count : Float) : Float := if count <= 17 then 37 else 629 / count
def mixY (index height : Float) : Float := 235 + index * height
def mixAt (y height : Float) : Float := Float.floor ((y - 235) / height)

/-- A level meter's filled width: `value` of `full`, within `width`. -/
def meter (value full width : Float) : Float := bounded (value / full) 0 1 * width
/-- Read a horizontal fader on its labelled range, rounded to hundredths. -/
def fader (x left width low high : Float) : Float :=
  Float.floor ((low + bounded ((x - left) / width) 0 1 * (high - low)) * 100 + 0.5) / 100
/-- Whole percent and hundredths, rounded to nearest. -/
def percent (value : Float) : Float := Float.floor (value * 100 + 0.5)
/-- [DAW-TRANSPORT-LIVE] Seconds of audio heard: the frame clock advanced by `elapsed`, held between what the
device has finished (`returned` frames) and what it was given (`accepted`), so the playhead waits when sound does. -/
def heardSeconds (clock elapsed returned accepted rate : Float) : Float :=
  bounded (clock + elapsed) (returned / rate) (accepted / rate)
/-- The beat sounding `seconds` into one pass from `first` over `span` beats; it stops at the end. -/
def passBeat (first span seconds tempo : Float) : Float := first + bounded (seconds * tempo / 60) 0 span
/-- The beat sounding `seconds` into a loop of `span` beats from `first`: each pass starts again at `first`. -/
def loopBeat (first span seconds tempo : Float) : Float :=
  let beats := seconds * tempo / 60
  first + (beats - Float.floor (beats / span) * span)
/-- The first bar of the two the piano roll shows while `beat` sounds: it turns the page every two bars from
the start marker's bar `start`. -/
def rollPage (beat start : Float) : Float :=
  if beat < start * 4 then start else start + Float.floor ((beat - start * 4) / 8) * 2
/-- [DAW-TRANSPORT-RANGE] The beat Play ends on: the end marker's bar line when it is after the start bar and
inside the song, else the song's end. -/
def rangeEnd (start finish beats : Float) : Float :=
  if finish > start && finish * 4 < beats then finish * 4 else beats
/-- The end marker's bar under beat `beat`: the nearest bar line, at least one bar after the start bar. -/
def endBar (beat start : Float) : Float :=
  let nearest := Float.floor (beat / 4 + 0.5)
  if nearest <= start then start + 1 else nearest
/-- The start marker's bar under beat `beat`: the nearest bar line, from the first bar to the bar before the end
marker (`finish` bar lines from 0, or 0 for the song's end) [DAW-TRANSPORT-RANGE]. -/
def startBar (beat finish beats : Float) : Float :=
  let whole := if beats <= 0 then 0 else Float.ceil (beats / 4) - 1
  let last := if finish > 0 && finish * 4 < beats then finish - 1 else whole
  bounded (Float.floor (beat / 4 + 0.5)) 0 last
/-- A marker's handle at `at` is grabbed by a press at `x`: within six units either side. -/
def grabs (x at : Float) : Bool := x >= at - 6 && x <= at + 6
