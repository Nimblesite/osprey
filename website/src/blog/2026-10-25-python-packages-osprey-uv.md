---
layout: page.njk
title: "Python Packages in Osprey: NumPy, PyTorch-Style Code, and uv"
excerpt: "Osprey's Python-flavor ports compile as Osprey packages. Real Python dependencies use uv. Here is how effects, scoped package names and verified locks connect the two."
description: "Understand Osprey's Python-flavor packages, NumPy and PyTorch-style ports, and the new package manager's verified source locks and uv integration for CPython dependencies."
date: 2026-10-25
tags: ["blog", "python", "packages", "numpy", "pytorch", "uv"]
author: "Christian Findlay"
readingTime: 5
image: /assets/images/blog/python-packages-osprey-uv.png
imageWebp: /assets/images/blog/python-packages-osprey-uv.webp
imageAlt: "A cyan wireframe osprey above glass modules holding a .py source file, a tensor grid and a dependency lock"
---

A numerical library should be able to describe its calculation while a handler chooses the storage. Osprey's Python-flavor NumPy port does that: array operations request a `Storage.Buffer` effect, and the caller installs either `Host.on_host` or `Device.on_gpu`.

The array code keeps the same source. The handler supplies host-list operations or Osprey's GPU operations. This is the reason to bring familiar Python-style code into Osprey: inferred static types and checked effects apply to the library as well as the application using it.

The new package manager gives those libraries scoped identities, verified source downloads and exact release locks. If an application also needs real Python libraries, its Python dependencies are resolved through uv.

## Python-flavor packages are compiled Osprey

The repository contains ports under names such as `@osprey/numpy` and `@osprey/torch`. Their `.py` files begin with:

```python
# osprey: flavor=python
```

That marker identifies Osprey source written in the [Python flavor](/docs/python-flavor/). It does not convert an arbitrary Python module into Osprey or make a PyPI distribution compile unchanged. The source uses familiar Python constructs alongside Osprey's types, effects and handlers.

The ports have defined scopes:

| Osprey package | Code in the repository | Boundary |
| --- | --- | --- |
| [`@osprey/numpy`](https://github.com/Nimblesite/osprey/tree/main/packages/numpy) | Float64 arrays, broadcasting, reductions, shape operations and linear algebra | Integer, object and complex arrays are outside the port. |
| [`@osprey/torch`](https://github.com/Nimblesite/osprey/tree/main/packages/torch) | Float64 tensor operations, autograd, optimizers and learning-rate schedules | Uses Osprey storage; it is not a binding to the PyTorch runtime or its complete operator catalogue. |
| [`@osprey/einops`](https://github.com/Nimblesite/osprey/tree/main/packages/einops) | Pattern parsing, transformation recipes and shape reconstruction | Does not expose the upstream tensor-backend execution API. |
| [`@osprey/safetensors`](https://github.com/Nimblesite/osprey/tree/main/packages/safetensors) | Headers, tensor metadata, byte decoding and serialization | Uses Osprey values and explicit file capabilities rather than Python tensor objects. |

These are source packages in the checkout, not a claim that a public catalog already hosts them. Check each package's README, implementation and tests against the operations your project needs. GPU placement is explicit; neither the file extension nor the package manager automatically accelerates a calculation.

To run one port's checks from the repository root:

```sh
./target/release/osprey packages/einops/tests --run
```

The default-backend run checked for this post finishes with `103 passed, 0 failed`, matching its recorded output. Those checks cover the port's parsing, recipes and shape reconstruction, not the upstream tensor-backend API.

## A manifest identifies the Python-flavor source

The [`@osprey/einops` manifest](https://github.com/Nimblesite/osprey/blob/main/packages/einops/osprey.toml) declares its source roots and Python-flavor modules:

```toml
[project]
name = "einops"
source_roots = ["src"]
python_sources = ["src/parsing.py", "src/recipe.py", "src/reconstruct.py"]
default_namespace = "einops"

[package]
name = "@osprey/einops"
license = "MIT"
summary = "einops pattern parsing and transformation recipes, pure Osprey"
keywords = ["einops", "rearrange", "reduce", "repeat", "tensor", "python-flavor"]

[test-dependencies]
"@osprey/testing" = { epoch = 1, min_release = 1 }
```

The registry checks that declared Python-flavor files are inside a source root and carry the marker. The source tree is hashed into a `SourceDigest`; downloaded bytes must match that identity before installation.

A dependency names an **epoch**, its compatibility lineage, and a minimum release ordinal. Resolution selects exact releases and pins them in `osprey.lock`. Ordinary build preparation verifies that lock and the installed trees instead of selecting new releases.

## Try the folder registry today

The registry server, client, manifest parser and resolver are written in Osprey. A local registry is a folder of validated releases; it needs no account or database.

From the repository root:

```sh
make build
./target/release/osprey build packages/manager/cli -o target/osprey-pkg
make registry-serve
```

Leave the server running. In another terminal, use the checked-in consumer to exercise resolution, installation and a locked build:

```sh
echo "lock --project $PWD/packages/manager/e2e/consumer --registry http://127.0.0.1:8787" | ./target/osprey-pkg
echo "fetch --project $PWD/packages/manager/e2e/consumer --registry http://127.0.0.1:8787" | ./target/osprey-pkg
echo "verify --offline --project $PWD/packages/manager/e2e/consumer" | ./target/osprey-pkg
echo "build --run --project $PWD/packages/manager/e2e/consumer" | ./target/osprey-pkg
```

That consumer demonstrates the registry flow with `@demo/report`; it does not install NumPy or PyTorch. The client currently reads commands from stdin and prints `status: N`. Check for `status: 0`; its process exit status does not yet carry the command's result.

The sample consumer uses Default-flavor source. Locked builds also accept Python-flavor sources; both flavors have passed real compiler checks for `build`, `build --run`, and relative or absolute `--project` paths. The client generates the compiler project inside the selected project's `.osprey/build/` directory.

For your own packages, the same client supports `publish --folder`, local checkout overrides and release updates. A folder server reads its catalog at startup, so restart it after publishing. The [package manager guide](/docs/package-manager/) covers those commands and their current limits.

## Where uv fits in Python dependency management

You can also keep a library running under CPython and call it through [Python interop](/spec/0042-pythoninterop/). This is separate from compiling an Osprey Python-flavor port.

A package that needs CPython declares `[system.python]` with the `python/interop` capability and a minimum CPython revision. Its Python project files describe its Python dependencies. Osprey then delegates that dependency resolution to uv:

1. **Lock:** compose a project under `.osprey/python/`, run `uv lock`, and pin the generated project and uv lock digests in `osprey.lock`.
2. **Fetch:** check the generated inputs against those pins before running `uv sync --frozen` to prepare the Python environment.
3. **Verify and build:** check the pinned files and composed project, run `uv lock --check --offline`, and require the environment to exist. These checks do not sync dependencies.

Osprey resolves Osprey packages; uv resolves the CPython requirements. A graph with no Python interop requirement does not invoke uv. Osprey does not install uv or CPython for you; `doctor` reports the relevant setup guidance.

If you usually reach for `pip install`, the first decision here is which code you want to run: an Osprey port compiled with the rest of your program, or a CPython library reached through the bridge. The names can be familiar while the runtime and supported API remain different.

## What the package website will add

The dedicated package website is planned for package discovery, release history, dependencies, capabilities and verification evidence. It will use the versioned Osprey registry API; Supabase is the planned hosted data layer behind that API. The working folder registry remains useful for local and private deployments.

Public authenticated publishing and the public trust services are still planned. Today's source digest verifies bytes, not publisher identity. Dependency capability grants constrain which releases can enter your graph; they do not establish that every library is benign. Osprey packages have no install hooks, while CPython environment installation remains uv's separate responsibility.

Osprey is actively evolving. Start with a small operation from your project, compare it with the Python result, and inspect the package's current scope. Use the [folder-registry quick start](/docs/package-manager/) to make that source dependency reproducible.
