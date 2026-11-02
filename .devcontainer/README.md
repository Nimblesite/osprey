# Osprey development container

The configuration builds a Debian bookworm workspace with Rust, the C toolchain, Node.js 20, benchmark tools and VS Code development extensions. The checkout is mounted at `/workspace`; the user is `vscode`, and ports 3001 and 8080 are forwarded.

## Current toolchain gap

[Dockerfile](Dockerfile) still installs clang/LLVM 14. Osprey's emitted allocator attributes require LLVM 15 or newer, as documented in the [web compiler image](../webcompiler/Dockerfile). The development image needs a toolchain update before its native build can be considered working.

[post-create.sh](post-create.sh) installs its configured CLI tools, runs `make setup` and attempts `make build`. It currently converts an initial build failure into a warning, so its completion message does not prove the compiler built. Run `make build` explicitly and inspect its exit status. Use the [installation guide](../website/src/docs/installation.md) for a compatible host toolchain.

## Open the workspace

With Docker, VS Code and the Dev Containers extension installed, open the repository and select **Dev Containers: Reopen in Container**.

The [container configuration](devcontainer.json) defines mounts, port forwarding, editor extensions and the post-create command. The image build and setup require network access.

## Build and verify

From `/workspace`, after resolving the toolchain gap:

```sh
make build
make test
make lint
```

The compiler lands at `target/release/osprey`. [build-compiler.sh](build-compiler.sh) wraps the root build. For extension development, run `npm run compile` from `/workspace/vscode-extension` and use its [test guide](../vscode-extension/TESTING.md).

Useful version checks are `rustc --version`, `cargo --version`, `clang --version`, `node --version` and `npm --version`. For setup failures, inspect the Docker build and post-create logs; a fresh container alone does not repair a pinned incompatible toolchain.
