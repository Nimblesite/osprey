/-! # Affine maps of `@osprey/math`

The Osprey module is generated from this file ([MATH-CODE]).
-/
namespace math.AffineMap

/-- The point a fraction `c` of the way from `a` to `b`: linear interpolation. Mathlib
`AffineMap.lineMap_apply_ring'`: `lineMap a b c = c * (b - a) + a`.
# Examples
```osprey
print "${AffineMap::lineMap (10.0, 20.0, 0.25)} ${AffineMap::lineMap (10.0, 20.0, 1.0)}"
```
```output
12.5 20.0
```
-/
def lineMap (a b c : Float) : Float := c * (b - a) + a

end math.AffineMap
