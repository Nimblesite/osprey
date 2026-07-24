# CLAUDE.md
<!-- agent-pmo:74cf183 -->

This file provides guidance for agents when working with code in this repository.

⚠️ NEVER ASK THE USER QUESTIONS! USE YOUR JUDGEMENT. ACT AUTONOMOUSLY ⚠️
⚠️ **NEVER DUPLICATE CODE** - Edit in place, never create new versions. Use deslop - find-similar before adding code, and deslop-top-offenders after modifying code ⚠️
⚠️ DO NOT USE GIT - ESPECIALLY NOT PUTTING YOUR SIGNATURE ON COMMITS ⚠️
⚠️ PRACTICE TOKEN ECONOMICS ⚠️
⚠️ ZERO DUPLICATE CODE ⚠️

## Core Development Principles

- **NO PLACEHOLDERS** - Fix existing placeholders or fail with error
- **RUN make ci ROUTINELY** - Many clippy lints can be easily fixed with auto fix. Don't try to fix them yourself
- **PREFER EXPANDING EXISTING EXAMPLES AND TESTS** - Don't add new examples/tests
- **DO NOT USE GIT - IN PARTICULAR, DO NOT STAMP YOURSELF AS COAUTHOR** - unless explicitly requested
- **MAKE EXAMPLES (TESTS) CONCISE AND MIX WITH MANY LANGUAGE CONSTRUCTS** - Don't create many files with overlapping functionality
- **KEEP ALL FILES UNDER 500 LOC** - Break large files into focused modules 
- **KEEP FUNCTIONS BELOW 20 LOC**
- **FP STYLE CODE** - pure functions over OOP style
- **Handle all panics and return Result<T,E>** instead of throwing
- **USE CONSTANTS** - Name values meaningfully instead of using literals

## Diagrams

⚠️ **ASCII ART DIAGRAMS ARE ILLEGAL** ⚠️ No box-drawing characters
(`┌─┐│└┘├┤▼►`), no `+---+` boxes, no hand-drawn arrow chains — in specs, plans,
READMEs, blog posts, or code comments. They are unreadable to screen readers,
break on reflow, and cannot be themed. Every diagram is one of exactly two
fenced languages, both of which the website renders:

- **Data types → ` ```typediagram `** — records, tagged unions, aliases. See
  the [language reference](https://typediagram.dev/docs/language-reference.html).
  Rendered to inline SVG at BUILD time; a diagram that fails to parse FAILS THE
  BUILD.
  ````
  ```typediagram
  type PackageKey { scope: String, name: String }
  union ReleaseState { Published { at: DateTime }  Yanked { reason: String } }
  ```
  ````
- **Everything else → ` ```mermaid `** — flow, sequence, state, architecture.
  Rendered in the browser from a vendored runtime (no CDN).

Only exception: directory trees (`├── crates/`) stay as text — a file listing
is not a diagram. Keep mermaid node labels SHORT (two lines max); long labels
clip. Diagram rendering is guarded by `website/tests/interactions.spec.js`.

## Rust

- **Panics are illegal** Return Result<T,E>
- **unwrap() and similar are illegal** Use pattern matching

## Osprey

- **Osprey is an FP language** - Use constructs that other FP languages use:
  - Immutable types
  - Expressions over statements
  - Avoid brackets where they are not necessary (ML style)
  - Use algebraic effects for abstractions
  - The best function is a single expression with no side effects (pure)
  - Avoid consecutive statements and assignments, even when assignments add
    clarity
- **LEAN ON TYPE INFERENCE — DO NOT WRITE REDUNDANT TYPE ANNOTATIONS** -
  Osprey is Hindley-Milner: every type the compiler can infer must be left
  off. This is a core style rule of the language — less redundancy.
  - **Never annotate function parameters** when their type is inferable from
    the body or call site. Write `fn add(a, b) = a + b`, NOT
    `fn add(a: int, b: int) = a + b`.
  - **Never annotate a function return type** when it is inferable. Write
    `fn isEven(x) = (x % 2) == 0`, NOT `fn isEven(x: int) -> bool = ...`.
  - **Never annotate lambda parameters** when inferable: `|x| => x * 2`, not
    `|x: int| => x * 2`.
  - Keep an annotation ONLY when the compiler genuinely cannot infer it: an
    empty literal with no context (`let xs: List<int> = []`), an `extern` /
    ambiguous return, an unconstrained polymorphic type variable, or a
    load-bearing return type that forces `Result<T, MathError>` to
    auto-unwrap to `T`. If removing an annotation still compiles and produces
    identical output, it was redundant — remove it.
  - This applies to ALL `.osp` you write or touch — `examples/tested/`,
    `benchmarks/`, docs, and website snippets alike.
- **NO CONSECUTIVE PRINT CALLS IN OSP** - Use string interpolation! Consolidate consecutive prints into singular interpolated strings!!!

## Commands

**Primary Development Commands (run from the repo root):**
```bash
make build         # C runtime archives + cargo build --release + VSCode extension
make test          # All tests + coverage thresholds + differential example harness
make lint          # cargo clippy + extension lint
make fmt           # Format all code in-place (CHECK=1 for read-only check)
make ci            # lint + test + build (full CI simulation)
make clean         # Clean all build artifacts
```

**Development Commands:**
```bash
make run FILE=<path>       # Compile and run an Osprey file (osprey <file> --run)
make install               # Install osprey + runtime archives system-wide
make _rebuild-install-vsix  # Rebuild + reinstall the VSCode extension (macOS)
```

The compiler binary lands at `target/release/osprey`.

**VSCode Extension:**
```bash
cd vscode-extension
npm install && npm run compile    # Build VSCode extension
npm test                         # Run extension tests
```

**Website Development:**
```bash
cd website
npm install && npm run dev       # Start local development server
npm run build                    # Build static site
```

**CSS HARD BUDGET 1.8K LOC** BLOGS, SPECS, DOCS = PROSE = SAME CSS NAME PREFIX PROSE
**ZERO TAILWIND** CONVERT TO CSS IMMEDIATELY

**NEVER COMMIT GENERATED FILES.** `website/src/spec/*.md` (from `docs/specs/`)
and `website/src/assets/vendor/` are build output — gitignored, regenerated by
`npm run build`. Edit the SOURCE (`docs/specs/`), never the copy.

**WebCompiler (Browser-based):**
```bash
cd webcompiler  
npm install && npm start         # Start web-based compiler service
```

## High-Level Architecture

**Repository Structure:**
- `crates/` - Core Osprey compiler (Rust workspace → LLVM)
- `tree-sitter-osprey/` - Tree-sitter grammar for parsing
- `compiler/` - Pure-C runtime sources (`runtime/`) + example programs (`examples/`)
- `vscode-extension/` - VSCode language support with TypeScript
- `website/` - Documentation site using 11ty static site generator
- `webcompiler/` - Node.js web service for browser compilation
- `homebrew-package/` - Homebrew tap for macOS installation

**Compiler Architecture (Rust-based):**
- **Parser**: Tree-sitter grammar (`tree-sitter-osprey/`) consumed by `crates/osprey-syntax`
- **AST**: Abstract Syntax Tree types in `crates/osprey-ast`
- **Type System**: Hindley-Milner type inference in `crates/osprey-types`
- **Code Generation**: LLVM IR generation in `crates/osprey-codegen`
- **CLI**: `crates/osprey-cli` (the `osprey` binary); `crates/osprey-runtime-sys` links the C runtime
- **Runtime**: C libraries (`compiler/runtime/`) for fiber concurrency, HTTP/WebSocket, system operations

**Language Features:**
- **Algebraic Effects**: First-class effects system with compile-time safety
- **Fiber Concurrency**: Lightweight isolated execution contexts
- **Pattern Matching**: Union types with exhaustiveness checking
- **Functional Programming**: Immutable data, pipe operators, iterators
- **HTTP/WebSocket**: Built-in networking with streaming support
- **Type Safety**: Strong static typing with inference

**Multi-Language Runtime:**
- **Rust**: Compiler frontend, parsing, type checking, codegen (`crates/`)
- **C**: Performance-critical runtime (fibers, HTTP, WebSockets) — stays C
- **LLVM IR**: Compilation target for optimized execution

**Key Technical Patterns:**
- Effects are declared with `effect` keyword and handled with `handle...in` expressions
- Unhandled effects cause compilation errors (world-first compile-time effect safety)
- Pattern matching is mandatory for `any` types and union types
- All HTTP/WebSocket operations return `Result<T, String>` for error handling
- Fiber isolation prevents shared memory bugs through message passing

**Testing Strategy:**
- Unit tests live inside each crate in `crates/`
- `examples/tested/` - Working examples run via the differential harness (`crates/diff_examples.sh`); output must match `.expectedoutput` byte-for-byte
- `examples/failscompilation/` - Error cases the compiler must reject
- Coverage thresholds enforced per-project via `coverage-thresholds.json`

**Security Architecture:**
- Configurable sandboxing for file access, HTTP, and process execution
- C runtime compiled with security hardening flags (`-D_FORTIFY_SOURCE=2`, `-fstack-protector-strong`)
- All warnings treated as errors in C compilation
- Effect system provides capability-based security

**Development Workflow:**
1. **Grammar Changes**: Edit the tree-sitter grammar in `tree-sitter-osprey/`
2. **Language Features**: Implement in `osprey-syntax`/`osprey-ast`, then `osprey-codegen`
3. **Testing**: Add examples to `examples/tested/` and error cases to `examples/failscompilation/`
4. **Type System**: Extend `crates/osprey-types` for new type rules
5. **Runtime**: Add C functions in `compiler/runtime/` for system operations

**AI-Assisted Development Notes:**
- This compiler is built using various AI agents and models
- AI can help with tree-sitter grammars, LLVM IR generation, and type inference
- The codebase follows clear patterns that AI can recognize and extend
- Use VS Code Dev Container for consistent development environment

This is a functional programming language compiler with algebraic effects, fiber-based concurrency, and strong compile-time safety guarantees.

## Standard Build Commands

```bash
make build        # Build compiler + VSCode extension
make test         # Fail-fast tests + coverage + threshold (coverage-thresholds.json)
make lint         # Run all linters (cargo clippy + extension lint)
make fmt          # Format all code in-place
make clean        # Remove build artifacts
make ci           # lint + test + build (full CI simulation)
make setup        # Post-create dev environment setup
```

## Releases & Versioning

- **Releases are tag-triggered only.** Push `vX.Y.Z` → `release.yml` builds all
  platforms, cuts the GitHub Release, updates the Homebrew tap + Scoop bucket,
  publishes the VS Code extension (`nimblesite.osprey`), and deploys the website.
- **CI runs only on PRs to `main`.** Merging to `main` triggers nothing.
- **Versions are NEVER hard-coded.** Every source version field stays at the
  placeholder `0.0.0-dev` (root `Cargo.toml` workspace version,
  `vscode-extension/package.json`, `shipwright.json`); the real version is
  stamped from the git tag at build time. Changing a placeholder to a real
  version in source is a defect. Follows the Shipwright contract
  ([SWR-VERSION-*]). See [docs/RELEASING.md](docs/RELEASING.md).

## Spec IDs

Spec IDs are hierarchical descriptive slugs in the format `[GROUP-TOPIC]` or `[GROUP-TOPIC-DETAIL]`. NEVER use numbered IDs (`[SPEC-001]`). Code implementing a spec section MUST reference its ID in a comment. Example: `// Implements [PARSER-EFFECTS-HANDLE]`.

## Branch Naming

| Type | Pattern | Example |
|------|---------|---------|
| Feature | `feature/[ISSUE]-[slug]` | `feature/42-add-pattern-matching` |
| Bug fix | `fix/[ISSUE]-[slug]` | `fix/17-null-ref-effects` |
| Chore | `chore/[slug]` | `chore/update-deps` |
| Claude agent | `claude/[slug]-[random5]` | `claude/refactor-XYZab` |

All changes via PR — no direct pushes to `main`. Squash-merge preferred.