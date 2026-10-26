# Source policy

## Authority order

The book uses the narrowest current authority available:

1. the user's edition-level direction and the book's editorial brief;
2. Osprey language specifications in `../docs/specs/`;
3. executable compiler, runtime, and corpus tests;
4. implementation code when a behavior needs confirmation;
5. maintained installation and status documentation;
6. `../docs/messaging.md` for product philosophy, with Default-first teaching as this edition's explicit flavor policy;
7. `../docs/designs/` and live website tokens for visual decisions.

The repository README is an orientation document, not the final authority when it disagrees with current specifications, tests, or edition direction.

## Flavor language

The book teaches Default first. Osprey currently has three syntax flavors: Default, ML and Python. A syntax flavor changes how source is written, while sharing types, effects and compilation.

Default and ML are the book's teaching surfaces. Python syntax is available for real programs, including the GPT-2 port; its supported subset and remaining frontend/tooling limits are documented in the [Python flavor guide](../website/src/docs/python-flavor.md). Distinguish Osprey Python-flavor modules from CPython packages, which require explicit interop. Do not expose compiler-internal vocabulary in the first chapters.

## Example evidence

Every complete example must:

1. live under `examples/`;
2. compile with the edition's pinned compiler;
3. produce deterministic output where output is claimed;
4. avoid undocumented behavior; and
5. appear in Default flavor before any alternative surface.

ML twins are verification aids and optional comparisons. They do not replace the Default source.

## Product and roadmap boundary

The book states that Osprey is actively evolving and separates implemented features from specifications of future work.

Project modules, multi-file assembly and imports across syntax flavors are implemented. The package client supports folder registries, verified downloads, locked builds, local overrides and folder publication; hosted authenticated publishing, the public package website and trust services remain planned. Use the [module specification](../docs/specs/language/0025-ModulesAndNamespaces.md) and its executable project tests for module claims, and the [package guide](../website/src/docs/package-manager.md) for available commands and limits. A specification alone does not prove a feature is available.

Qualify runtime claims where they are taught:

- Native dynamic control handlers currently provide single-shot resumption. WebAssembly and mobile C ABI targets support value handlers and static discharge, and reject unsupported dynamic control operations.
- Native compilation defaults to `gc`; `arc` selects reference counting. Both have platform limits. Explicit `--memory=none` selects unmanaged allocation and retains general allocations. WebAssembly and mobile libraries currently require this option because managed runtime archives are unavailable; the compiler does not substitute it silently.
- Normal native fibers use one operating-system thread each and can co-own immutable allocations. Structured scopes, fiber cancellation, `join`, and deadlines remain design work.
- Strict static-memory checking remains unimplemented. GPU device support is kernel- and target-dependent; consult plan 0023 and the runtime implementation rather than assuming all work runs on the host or on a device.

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
