# Source policy

## Authority order

The book uses the narrowest current authority available:

1. the user's edition-level direction and the book's editorial brief;
2. Osprey language specifications in `../docs/specs/`;
3. executable compiler, runtime, and corpus tests;
4. implementation code when a behavior needs confirmation;
5. maintained installation and status documentation;
6. `../docs/messaging.md` for product philosophy, except where this edition's explicit flavor policy supersedes its fixed-count framing;
7. `../docs/designs/` and live website tokens for visual decisions.

The repository README is an orientation document, not the final authority when it disagrees with current specifications, tests, or edition direction.

## Flavor language

The book teaches Default first. ML is one currently available optional alternative, and more flavors may arrive later. Statements such as “Osprey has exactly two flavors” are forbidden in forward-looking editorial copy.

When a current command or table needs a count, write “the currently available Default and ML flavors.” Explain that a flavor owns source spelling while shared checking and code generation operate on the lowered program. Do not expose compiler-internal vocabulary in the first chapters.

## Example evidence

Every complete example must:

1. live under `examples/`;
2. compile with the edition's pinned compiler;
3. produce deterministic output where output is claimed;
4. avoid undocumented behavior; and
5. appear in Default flavor before any alternative surface.

ML twins are verification aids and optional comparisons. They do not replace the Default source.

## Product and roadmap boundary

The book states that Osprey is alpha software. It does not describe planned generics completion, package management, complete multi-file imports, strict static memory, device GPU code generation, or unsupported WebAssembly runtime services as shipped.

When source, specification, implementation, and tests disagree, the book omits the disputed behavior from learner-facing instruction and records the gap in `evidence.json`.

## Visual evidence

- Deterministic SVG diagrams explain concepts and may contain exact code or labels.
- Direct screenshots show the compiler, Playground, editor, or other product surfaces.
- Generated editorial illustration establishes mood only and contains no factual text.
- Every ready visual has dimensions, alt text, provenance, and a matching `figures.json` entry.

## Edition maintenance

Before publishing an edition:

1. set the compiler version and build date in `book.json` and `metadata.yaml`;
2. run every example with that compiler;
3. compare chapter claims with the cited specifications and tests;
4. render and inspect every figure at desktop and 320 px width;
5. run `make release`; and
6. record unresolved limits beside the relevant feature.

