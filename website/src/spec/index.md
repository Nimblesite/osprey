---
layout: page
title: "Osprey Language Specification"
description: "Complete language specification and syntax reference for the Osprey programming language"
date: 2026-07-22
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/"
---

# Osprey Language Specification

**Version:** 0.2.0  
**Date:** 2026-07-22  
**Author:** Christian Findlay

## Table of Contents

1. [Introduction](/spec/0001-introduction/)
2. [Lexical Structure](/spec/0002-lexicalstructure/)
3. [Syntax](/spec/0003-syntax/)
4. [Type System](/spec/0004-typesystem/)
5. [Function Calls](/spec/0005-functioncalls/)
6. [String Interpolation](/spec/0006-stringinterpolation/)
7. [Pattern Matching](/spec/0007-patternmatching/)
8. [Block Expressions](/spec/0008-blockexpressions/)
9. [Boolean Operations](/spec/0009-booleanoperations/)
10. [Iterators and Iteration](/spec/0010-loopconstructsandfunctionaliterators/)
11. [Fibers and Concurrency](/spec/0011-lightweightfibersandconcurrency/)
12. [Built-in Functions](/spec/0012-built-infunctions/)
13. [Error Handling](/spec/0013-errorhandling/)
14. [HTTP](/spec/0014-http/)
15. [WebSockets](/spec/0015-websockets/)
16. [Security and Sandboxing](/spec/0016-securityandsandboxing/)
17. [Algebraic Effects](/spec/0017-algebraiceffects/)
18. [Memory Management](/spec/0018-memorymanagement/)
19. [Foreign Function Interface](/spec/0019-foreignfunctioninterface/)
20. [Language Server & Editor Integrations](/spec/0020-languageserverandeditors/)
21. [Debugger](/spec/0021-debugger/)
22. [WebAssembly Target](/spec/0022-webassemblytarget/)
23. [Language Flavors](/spec/0023-languageflavors/)
24. [ML Flavor Syntax](/spec/0024-mlflavorsyntax/)
25. [Modules and Namespaces](/spec/0025-modulesandnamespaces/)
26. [Documentation Comments](/spec/0026-documentationcomments/)
27. [Testing Framework](/spec/0027-testingframework/)
28. [0028 — CPU Profiler](/spec/0028-profiler/)

## About This Specification

This specification defines the complete syntax and semantics of the Osprey programming language. Each section is available as a separate page for easy navigation and reference.

The Osprey language is designed for elegance, safety, and performance, emphasizing:

- **Typed algebraic effects** with lexical handlers; complete static coverage checking remains in progress
- **Named arguments** for multi-parameter functions to improve readability
- **Strong type inference** (Hindley-Milner) to reduce boilerplate while maintaining safety
- **String interpolation** for convenient text formatting
- **Pattern matching** for elegant conditional logic
- **Immutable-by-default** variables and persistent collections
- **Fast HTTP/HTTPS servers and clients** with built-in streaming support
- **C interoperability** via a typed foreign function interface

## Implementation Status

🚧 **NOTE**: The Osprey language and compiler are actively under development. This specification represents the design goals and planned features. Please refer to individual sections for current implementation status.
