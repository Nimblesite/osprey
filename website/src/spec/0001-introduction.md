---
layout: page
title: "Introduction"
description: "Osprey Language Specification: Introduction"
date: 2026-07-15
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/0001-introduction/"
---

# Introduction

Osprey is a statically-typed functional language in the ML family. It compiles to native code via LLVM.

> **Flavor layer — mixed.**  Osprey is one language core fronted by more than one source surface, called *flavors*. There is exactly **one AST** — the canonical `osprey_ast::Program` — but many concrete surfaces (CSTs). Two flavors exist today: the Default flavor (`.osp`), with C-style braces and named-argument calls, described by specs 0001–0022 here; and the ML flavor (`.ospml`), with offside-rule layout and whitespace application, described by [ML Flavor Syntax](/spec/0024-mlflavorsyntax/). Both lower to the same `osprey_ast::Program` before any semantic analysis, so type inference, effect checking, and codegen never see which flavor produced a program. The full model and the surface/shared-core boundary are defined in [Language Flavors](/spec/0023-languageflavors/).

## Core Features

- Hindley-Milner type inference; explicit annotations are optional.
- Pattern matching as the only conditional construct (no `if`/`else`).
- Immutable bindings by default; `mut` opts in to mutability.
- Algebraic effects checked at compile time.
- `Result<T, E>` for all fallible operations; no exceptions, panics, or null.
- In the Default flavor, named arguments are required for functions of two or more parameters (`f(x: a, y: b)`); the ML flavor uses whitespace application (`f a b`) or the uncurried grouping (`f (x, y)`) instead.
- Lightweight fibers and channel-based concurrency.
- Automatic memory management with no observable collector — ARC by default, tracing GC selectable, and a `--static-memory` mode with zero runtime memory operations.
- Built-in HTTP and WebSocket support.

## Status

This specification is the authoritative source for Osprey syntax and behaviour. The language and compiler are under active development; implementation status is called out per chapter where it diverges from the specification.