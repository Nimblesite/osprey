/-! The round dial of [DAW-CONTROLS-DIAL]: where a value sits on its arc, and what a vertical drag makes of it. -/
namespace synth.DialMath

private def bounded (value low high : Float) : Float :=
  if value < low then low else if value > high then high else value

/-- How far round its dial a value is: 0 at the minimum, 1 at the maximum. A range that is all positive and spans
more than a hundredfold (a frequency, a time) is turned on a logarithmic scale, so its small values get as much
travel as its large ones; any other range is turned evenly. -/
def turn (value low high : Float) : Float :=
  if high <= low then 0
  else if low > 0 && high / low > 100 then
    bounded (Float.log (bounded value low high / low) / Float.log (high / low)) 0 1
  else bounded ((value - low) / (high - low)) 0 1

/-- The value at a turn: the inverse of `turn`. -/
def valueAt (turned low high : Float) : Float :=
  if high <= low then low
  else if low > 0 && high / low > 100 then low * Float.exp (bounded turned 0 1 * Float.log (high / low))
  else low + bounded turned 0 1 * (high - low)

/-- A drag that began at `start`: lifting the pointer 180 units crosses the whole range, a tenth as fast when
`fine` (Shift) is held. The result is rounded to four significant places of its range so it reads cleanly. -/
def dragged (start lift low high fine : Float) : Float :=
  let pace := if fine > 0.5 then 0.1 else 1
  let value := valueAt (turn start low high + lift / 180 * pace) low high
  let grain := if high - low > 1000 then 1 else if high - low > 20 then 10 else if fine > 0.5 then 1000 else 100
  bounded (Float.floor (value * grain + 0.5) / grain) low high

/-- A press becomes a drag once the pointer has moved two units up or down; sideways movement never turns. -/
def moved (startY y : Float) : Float := if Float.abs (y - startY) >= 2 then 1 else 0

/-- The dial sweeps three quarters of a circle, from lower left over the top to lower right. This is the angle of
a turn in radians, clockwise from straight up. -/
def angle (turned : Float) : Float := (bounded turned 0 1 * 1.5 - 0.75) * 3.141592653589793

/-- The point at `radius` from the centre on the dial's arc at a turn. -/
def tipX (centre radius turned : Float) : Float := centre + radius * Float.sin (angle turned)
def tipY (centre radius turned : Float) : Float := centre - radius * Float.cos (angle turned)

/-- The row a height falls in when rows are `pitch` apart and the first is centred on `top`. -/
def row (y top pitch : Float) : Float := Float.floor ((y - top) / pitch + 0.5)

