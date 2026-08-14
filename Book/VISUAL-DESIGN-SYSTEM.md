# Visual design system — First Flight

## Creative north star

The book adapts Osprey's **Midnight Synthetic** design language into **First Flight**: a calm technical field guide in which small pieces of source become visible, trustworthy programs.

The visual narrative has three verbs:

- **Compose** — small values and functions join without clutter.
- **Check** — a cyan flight path passes through explicit compiler gates.
- **Launch** — one source file becomes a native or WebAssembly program.

The result should feel precise, nocturnal, optimistic, and welcoming. It must not resemble a cyberpunk game menu, a generic AI dashboard, or a children's activity book.

## Evidence hierarchy

1. **Runnable source and direct captures** prove what Osprey does.
2. **Deterministic diagrams** explain syntax, data flow, and program structure.
3. **Generated editorial illustration** may open a part or express the flight metaphor without carrying facts.

Generated imagery never owns code, commands, diagnostics, labels, or learning-critical arrows.

## Palette

The book is dark-first to match the website and uses tonal layers instead of heavy borders.

| Role | Hex | Use |
|---|---|---|
| Midnight canvas | `#070d1f` | Cover, page edges, code recesses |
| Reading surface | `#0c1325` | Primary EPUB and HTML background |
| Low surface | `#151b2d` | Notes and alternate sections |
| Container | `#191f32` | Quiet grouped evidence |
| Raised surface | `#23293d` | Tables, diagram nodes, code headers |
| Primary text | `#dce1fb` | Body and headings |
| Muted text | `#bdc8cd` | Captions, metadata, secondary explanation |
| Flight cyan | `#77d7f4` | Connections, focus, interactive meaning |
| Bright cyan | `#bdeeff` | Hot highlights and key terms |
| Periwinkle | `#bbc5ec` | Rare alternative-surface cue |
| Warm checkpoint | `#ffbe65` | Rare caution or deliberate reader action |
| Error | `#ffb4ab` | Compiler rejection only |

Cyan is the only energetic field color. Amber marks a reader checkpoint, never decoration. Error red appears only where a rejected program is the lesson.

## Long-form typography

The website specifies Geist and JetBrains Mono but does not define a book reading measure or print fallback. This book fills that gap:

- Display and headings: Geist, Inter, or an EPUB-safe system sans
- Body: Geist, Inter, or a system sans at 1.68–1.78 line height
- Code and utility labels: JetBrains Mono, SFMono-Regular, Consolas, or monospace
- Maximum prose measure: 68 characters, approximately 720 px on HTML
- Minimum body size: 1 rem / reader default; never lock EPUB text below that
- Eyebrows: uppercase mono, 0.08 em tracking
- Code: 0.86–0.9 em with 1.55 line height and visible wrapping policy

Headings use tight tracking. Body text stays left aligned. Fully justified text is forbidden because uneven word spacing harms younger and dyslexic readers.

## Layout and rhythm

- Use an 8 px base spacing unit.
- Keep one primary reading column.
- Give chapters a 64–96 px opening field on large screens.
- Put optional details after the main explanation, never beside the first code sample.
- Prefer background shifts and whitespace over outlined cards.
- Corners are 6–10 px to match the website tokens; code blocks use 8 px.
- Use no pills except genuine short metadata tags.

## Learning components

### Try it

A cyan-led exercise with one requested edit, one prediction, and one command to run. It must be possible to finish in under ten minutes.

### Compiler says

An error surface that highlights the source location, the expected idea, and the next useful action. Do not reproduce a diagnostic unless it came from the pinned compiler.

### Under the wing

An optional note for readers who want the precise functional-programming term. It never contains knowledge required by the next section.

### Same flight, different feathers

An optional flavor comparison. Default appears first and receives the full explanation. Alternative syntax is smaller, clearly dismissible, and paired with a verification command.

### Agent handoff

A paste-ready prompt on a raised surface. It includes the intended change, invariants, and the exact check or test the agent must run.

## Diagram language

| Idea | Form |
|---|---|
| Source value | Small cyan node with exact label |
| Pure function | Open rectangular transform with input/output ports |
| Pipeline | One continuous rising cyan path |
| Inferred type | Muted annotation attached after the value, not before it |
| Pattern match | Explicit fan-out with every branch visible |
| Result | Two named routes: success and error |
| Effect | Dashed request rising to a solid handler boundary |
| Fiber | Separate parallel flight path with no shared node |
| Compiler check | Thin luminous gate across the path |
| Flavor | A source-side feather shape that converges before checking |
| Output target | Native or WebAssembly landing field |

Arrows always name what moves. A box labelled only “magic,” “compiler,” or “quality” teaches nothing.

## Canvas families

| Asset | Master | Publication derivative |
|---|---|---|
| Cover | 1600 × 2560 SVG | 1600 × 2560 PNG |
| Concept diagram | 1600 × 1000 SVG | 1600 × 1000 PNG |
| Editorial opener | 16:9 raster | 1600 × 900 PNG/WebP |
| Product capture | Native high-DPI capture | 1600 px-wide crop where practical |

Keep at least 72 px safe margin around deterministic diagram content and 8% around raster focal elements. Publication assets are opaque.

## Cover direction

The cover is a midnight launch field. A single cyan program line rises through three checking planes and becomes an abstract osprey flight path. The canonical Osprey mark is composited from `../website/src/assets/images/logo.png`; it is never redrawn by a generative model.

Required text:

```text
THE OSPREY BOOK
A practical first flight through modern programming.
```

The title must remain readable at 160 px. No fake editor, terminal, source listing, laptop, human figure, or generated lettering.

## Generated-art prompt contract

Generated editorial art uses the repository's Midnight Synthetic image guidance with these book additions:

- one clear flight metaphor per image;
- calm negative space rather than dense spectacle;
- no people, laptops, fake code, UI, or text;
- abstract wireframe osprey anatomy must still read as a broad-winged raptor;
- cyan `#77d7f4` is the only vibrant color; and
- a chapter image must remain legible at a 320 px e-reader width.

## Accessibility and production gates

Every ready visual must have:

- descriptive alt text explaining the lesson;
- a caption explaining why the figure exists;
- readable content at 320 px width;
- sufficient contrast and a grayscale-safe distinction;
- no information carried by color alone;
- a source master and exact dimensions;
- no personal paths, secrets, or private repository names;
- no fictional product output; and
- a matching entry in `figures.json`.

