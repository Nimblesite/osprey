# @osprey/graphics

Osprey's core graphics primitives ([UI-SCENE]). A scene is a flat list of `Primitive` values in the application's own units, drawn by a host in one pass per layer, lower `depth` first. The application owns every position, colour and rule; the host owns the pixels.

| Type | What it is |
| --- | --- |
| `Primitive` | `Rect`, `Disc`, `Strip` (a polyline), `Sprite` (an atlas image), `Glyphs`, `Lettering` (anchored, weighted text) and `Path` |
| `PathStep` | The steps of a path: `MoveTo`, `LineTo`, `CurveTo`, `CubicTo`, `ArcTo`, `Polygon` and `Close` |
| `Outline`, `Paint`, `Tint` | A path's stroke, whether it is filled or lined, and an `Rgb` or `Hsl` colour |
| `Scenes` | Constructors for each primitive, and `hex`, `rgba` and `rgb` for packed `0xRRGGBBAA` colours |

`@osprey/native` puts a scene in a `Scene` node and encodes it for every host; a UI machine's canvas skin ([UI-MACHINE-CANVAS](../../docs/specs/platforms/UiMachines.md)) renders its shapes to these primitives. A project that uses the native framework lists both source roots:

```toml
source_roots = ["src", "../../packages/native/src", "../../packages/graphics/src"]
```

The types live in the `ui` namespace, so `ui::Scenes::rect` and `ui::Primitive` read the same as before the package was split out of `@osprey/native`.
