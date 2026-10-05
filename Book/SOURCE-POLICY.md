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

The book states that Osprey is alpha software and separates implemented features from specifications of future work.

Project modules, multi-file assembly, and imports across Default and ML flavors are implemented. The package registry and manager remain planned. Use the [module specification](../docs/specs/0025-ModulesAndNamespaces.md) and its executable project tests for module claims; a package-manager specification is not evidence of an available command.

Qualify runtime claims where they are taught:

- Native dynamic control handlers currently provide single-shot resumption. WebAssembly and mobile C ABI targets support value handlers and static discharge, and reject unsupported dynamic control operations.
- The default allocator retains general allocations. Native `gc` and `arc` are separate choices with platform limits; neither is the default, and neither is currently available for WebAssembly or mobile libraries.
- Normal native fibers use one operating-system thread each and can co-own immutable allocations. Structured scopes, fiber cancellation, `join`, and deadlines remain design work.
- Strict static-memory checking and GPU device execution remain unimplemented. Current GPU kernels run through host loops.

Check these boundaries against the relevant specifications, implementation tests, and delivery plans before describing a feature as available. The [messaging guide](../docs/messaging.md) links the maintained status sources; it does not replace executable evidence.

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
