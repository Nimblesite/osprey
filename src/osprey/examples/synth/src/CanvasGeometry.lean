/-! Canvas placement and pointer offsets [PRISM-ROUTING-LAYOUT]. -/
namespace synth.CanvasGeometry

/-- Default horizontal centre for one grid column. -/
def gridX (column : Float) : Float := 100 + column * 210

/-- Default vertical centre for one grid row. -/
def gridY (row : Float) : Float := 65 + row * 130

/-- The pointer's displacement from the grabbed card's centre. -/
def offset (pointer centre : Float) : Float := pointer - centre

private def bounded (value low high : Float) : Float :=
  if value < low then low else if value > high then high else value

/-- Keep the card and port hit targets within the horizontal canvas bounds. -/
def dragX (pointer grab : Float) : Float := bounded (pointer - grab) 95 755

/-- Keep the entire card within the vertical canvas bounds. -/
def dragY (pointer grab : Float) : Float := bounded (pointer - grab) 40 560

/-- NaN, infinity and excessive pointer coordinates never enter saved layout. -/
def valid (px py : Float) : Bool :=
  px == px && py == py && Float.abs px <= 1000000 && Float.abs py <= 1000000

end synth.CanvasGeometry
