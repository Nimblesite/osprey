# Contributing to Osprey

Osprey is in stabilization mode. Contributions should harden existing behavior, improve diagnostics and tests, or correct documentation. Use the runnable applications and active plans to identify gaps before proposing new language features.

**Compiler changes**: always create an issue before starting work on the compiler. Explain the issue and propose a solution for the issue. Please get approval before moving into the implementation phase. All contributions are appreciated but compiler work requires strict review and planning.

Start with the [repository map](docs/contributing/repository-map.md) to find the component and build rules for your change.

## The Tech Stack

The compiler and runtime use:

- **[Rust](https://www.rust-lang.org/)** - Compiler implementation (`crates/`)
- **[tree-sitter](https://tree-sitter.github.io/)** - Grammar parsing (`tree-sitter-osprey/`)
- **[LLVM](https://llvm.org/)** - Code generation (textual IR handed to clang)
- **C** - The runtime (fibers, HTTP/WebSocket, collections) in `compiler/runtime/`

## Getting Started

Start with the [installation guide](website/src/docs/installation.md) for host toolchain prerequisites. The [development-container guide](.devcontainer/README.md) records its configured environment and current LLVM/setup limits.

```bash
# Fork and clone
git clone https://github.com/Nimblesite/osprey.git
cd osprey

# Build and test (from the repo root)
make build
make test
```

## Repository coordination

Git worktrees are banned. There may be only **ONE active feature branch across ALL machines and clones combined**. Coordinate all work on the existing active branch; do not create a branch per machine, contributor, or task. A replacement branch may start only after the previous branch is merged or explicitly retired and all machines are coordinated. See [CLAUDE.md](CLAUDE.md) for the shared instructions.

## What to Work On

Osprey is in stabilization mode. Prioritize reliability of existing features, truthful diagnostics, and conformance to the documented contract. Active [plans](docs/plans/README.md) distinguish unfinished requirements from shipped behavior.

1. Harden existing language features and built-ins.
2. Expand acceptance and rejection coverage for edge cases.
3. Reconcile specs, code, tests, plans, and public status claims when behavior changes.
4. Improve existing examples and application boundaries.

## The AI Workflow

1. **Check the contract** — Read the relevant spec, implementation status, tests, and active plan.
2. **Understand the pattern** - Ask your AI: "How does pattern matching work in this codebase?"
3. **Pin the failure first** — Extend an existing acceptance or rejection test before changing implementation.
4. **Follow the pipeline** — Parsing, AST, type/effect checking, code generation and runtime must agree; run the relevant crate tests and differential corpus.
5. **Fix edge cases** - Let the AI help debug LLVM IR issues

Example prompts:

- "Add a regression for this incorrect arithmetic result before fixing it."
- "Verify that this effect requirement survives a helper and callback."
- "Check this guide against the current CLI and its acceptance tests."

## Code Guidelines

- Follow existing patterns - the clippy lint wall (`make lint`) enforces a lot
- Test new features thoroughly
- Keep changes focused
- Fix linter errors before submitting
- **Never hard-code release versions.** The release-managed compiler, extension and Shipwright version fields stay at `0.0.0-dev`; releases stamp them from the Git tag. See [docs/RELEASING.md](docs/RELEASING.md) for the exact fields.

## CI & Releases

- **CI runs on PRs to `main`** (`ci.yml` and `ci-windows.yml`); Windows also supports manual dispatch. Merging to `main` does not trigger these workflows.
- **Releases are tag-triggered.** `release.yml` uses the tag diff to choose artifact builds, package publication and website deployment. Website-only changes skip the binary matrix; every publishing path still depends on a fresh dependency audit. See [docs/RELEASING.md](docs/RELEASING.md).

## Getting Help

- Open an issue for discussion
- Check existing issues and documentation
- Review similar implementations in the codebase
- The spec is the source of truth

## License

By contributing, you agree that your contributions will be licensed under the MIT License. 
