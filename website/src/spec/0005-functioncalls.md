---
layout: page
title: "Function Calls"
description: "Osprey Language Specification: Function Calls"
date: 2026-07-22
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/0005-functioncalls/"
---

# Function Calls

> **Flavor layer — surface (CST) only.**  Flavors differ purely in surface spelling; the semantics and lowering are **shared-core** and flavor-blind — both flavors parse to one canonical `osprey_ast::Program` at [FLAVOR-BOUNDARY](/spec/0023-languageflavors/), and the arity, named-argument, and saturation rules below are enforced by the type checker on that shared AST. This chapter shows **both** flavors: the **Default** (`.osp`) spelling, and — where the surface differs — the **ML** (`.ospml`) twin inline alongside it (```osprey-ml blocks). The Default call spellings — named-argument `f(x: a, y: b)`, positional `f(x)`, and `f()` — each lower to a single canonical node, `Expr::Call { function, arguments, named_arguments }`. The **ML** flavor (`.ospml`) writes calls two ways: whitespace application `f a b` ([FLAVOR-ML-CALL](/spec/0024-mlflavorsyntax/)), which **curries by default** and lowers to nested one-argument `Expr::Call`s (`Call(Call(f, [a]), [b])`); and the **uncurried** call `f (a, b)` — parentheses around a comma-list — which lowers to a single multi-argument `Call(f, [a, b])`, the exact twin of the Default named-argument call `f(x: a, y: b)`. (The parenthesised comma-list is argument grouping, not a tuple — Osprey has no tuple type.) The one honest surface difference is currying ([FLAVOR-CURRY](/spec/0023-languageflavors/#currying-canonicalisation)). See [Language Flavors](/spec/0023-languageflavors/) and [ML Flavor Syntax](/spec/0024-mlflavorsyntax/).

## Named Arguments Requirement

Functions with more than one parameter must be called with named arguments.

### Valid Function Calls

```osprey
// Zero parameters
fn getValue() = 42
let value = getValue()

// Single parameter - positional allowed
fn double(x) = x * 2
let result = double(5)

// Multiple parameters - named arguments required
// (Default surface; lowers to one multi-arg Expr::Call. The ML twin of this flat fn add(x, y)
// is the uncurried add (10, 20); whitespace add 10 20 is the curried twin of the explicit-curry
// def fn add(x) = fn(y) => x + y — a DIFFERENT value.)
fn add(x, y) = x + y
let sum = add(x: 10, y: 20)

// Order doesn't matter with named arguments
let sum2 = add(y: 20, x: 10)

// Multi-parameter definition + named-argument call
fn multiply(a, b) = a * b
let product = multiply(a: 5, b: 3)
```

```osprey-ml
// Zero parameters
getValue () = 42
value = getValue ()

// Single parameter - positional allowed
double x = x * 2
result = double 5

// Multiple parameters - uncurried tuple call (twin of the flat fn add(x, y))
// (whitespace add 10 20 is the curried twin of the explicit-curry def
// add x = \y => x + y — a DIFFERENT value.)
add (x, y) = x + y
sum = add (10, 20)

// Order is positional in the uncurried form
sum2 = add (10, 20)

// Multi-parameter definition + uncurried call
multiply (a, b) = a * b
product = multiply (5, 3)
```

### Invalid Function Calls

```osprey
// ERROR: Multi-parameter function with positional arguments
fn add(x, y) = x + y
let sum = add(10, 20)  // Compilation error

// ERROR: Mixed positional and named arguments
let sum = add(10, y: 20)  // Compilation error

// ERROR: Missing parameter name
let result = multiply(5, b: 3)  // Compilation error
```

## Rules

1. Zero parameters: empty parentheses, `f()`.
2. One parameter: positional or named.
3. Two or more parameters: every argument must be named. Mixing positional and named arguments is a compilation error.
4. **Built-in functions** ([Built-in Functions](/spec/0012-built-infunctions/)) are exempt: they take positional arguments in subject-first order — `split("a,b,c", ",")`, `fold(xs, 0, add)` — so the pipe can supply the subject as the first argument: `xs |> fold(0, add)`.

Argument order at the call site is independent of declaration order; the compiler reorders by name.

These rules are enforced on the canonical `Expr::Call` after lowering, so they hold identically regardless of flavor; the type checker is flavor-blind. ML whitespace application **curries by default** — `add 10 20` lowers to nested one-argument calls, each saturated against a one-parameter function — see [FLAVOR-CURRY](/spec/0023-languageflavors/#currying-canonicalisation) and [ML Flavor Syntax](/spec/0024-mlflavorsyntax/).

The two flavors twin call-for-call: a Default named-argument call `add(x: 10, y: 20)` (against a flat multi-parameter `fn add(x, y)`) is written in ML as the uncurried `add (10, 20)`; a Default explicit-curry call `add(10)(20)` is written in ML as whitespace `add 10 20`. Each pair lowers to the same `Expr::Call` shape and emits byte-identical IR.

## Cross-references

- [Language Flavors](/spec/0023-languageflavors/)
- [ML Flavor Syntax](/spec/0024-mlflavorsyntax/)