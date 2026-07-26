# Contributing to Osprey

Want to help build a programming language? You're in the right place. Building a language is a several year long process. Right now, the aims are building community, getting the first example apps deployed and shaping the ergonomics of the language.

Even discussions are great at this point.

**CRITICIAL**: always create an issue before starting work on the compiler. Explain the issue and propose a solution for the issue. Please get approval before moving into the implementation phase. All contributions are appreciated but compiler work requires strict review and planning.

## The Tech Stack

Osprey is built on battle-tested tools:
- **[Rust](https://www.rust-lang.org/)** - Compiler implementation (`crates/`)
- **[tree-sitter](https://tree-sitter.github.io/)** - Grammar parsing (`tree-sitter-osprey/`)
- **[LLVM](https://llvm.org/)** - Code generation (textual IR handed to clang)
- **C** - The runtime (fibers, HTTP/WebSocket, collections) in `compiler/runtime/`

## AI Assisted Development

You don't need a CS degree to implement language features. I've been using Claude Sonnet 4 with Cursor, and it's the first combo that actually guided me through the process of building a compiler. Other AI agents will work too, but this setup lets anyone contribute to language design.

The AI can help you:
- Extend the tree-sitter grammar and understand AST patterns
- Implement new operators and language constructs
- Debug LLVM IR generation
- Write comprehensive tests

## Getting Started

**Use VS Code Dev Containers** - strongly recommended. Open in VS Code and hit "Reopen in Container". Everything's already configured.

```bash
# Fork and clone
git clone https://github.com/Nimblesite/osprey.git
cd osprey

# Build and test (from the repo root)
make build
make test
```

## What to Work On

1. **Language features** - Check the [language specification](docs/specs/) for "NOT IMPLEMENTED" 
2. **New operators** - Add arithmetic, comparison, or logical operators
3. **Pattern matching** - Extend match expressions
4. **Standard library** - Add built-in functions
5. **Make examples** - The HTTP server works. Try building an API
6. **Test compilation errors** - Make sure the compiler is forcing you to do things the right way

## The AI Workflow

1. **Understand the pattern** - Ask your AI: "How does pattern matching work in this codebase?"
2. **Implement incrementally** - Start with parsing, then AST, then codegen
3. **Test everything** - Add assertion suites to `tests/`
4. **Fix edge cases** - Let the AI help debug LLVM IR issues

Example prompts:
- "Add a new arithmetic operator to the tree-sitter grammar"
- "Implement string interpolation in the AST builder"
- "Generate LLVM IR for this pattern match"

## Code Guidelines

- Follow existing patterns - the clippy lint wall (`make lint`) enforces a lot
- Test new features thoroughly
- Keep changes focused
- Fix linter errors before submitting
- **Never hard-code a version.** All version fields stay at `0.0.0-dev` in source;
  releases stamp the real version from the git tag. See [docs/RELEASING.md](docs/RELEASING.md).

## CI & Releases

- **CI runs only on PRs to `main`** (`ci.yml` + `ci-windows.yml`). Merging to
  `main` triggers nothing.
- **Releases are tag-triggered**: push a tag `vX.Y.Z` and `release.yml` builds
  every platform, cuts the GitHub Release, updates the Homebrew tap + Scoop
  bucket, publishes the VS Code extension, and deploys the website. Full details
  in [docs/RELEASING.md](docs/RELEASING.md).

## Getting Help

- Open an issue for discussion
- Check existing issues and documentation
- Review similar implementations in the codebase
- The spec is the source of truth

## License

By contributing, you agree that your contributions will be licensed under the MIT License. 
