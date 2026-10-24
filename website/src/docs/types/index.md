---
layout: page
title: "Built-in Types"
description: "Reference for core value types and runtime handles in Osprey"
---

These guides cover common values and runtime handles. The [type-system reference](/spec/0004-typesystem/#built-in-types) also covers `Result`, lists, maps, functions, records and unions. [Fibers](/spec/0011-lightweightfibersandconcurrency/) and [GPU buffers](/spec/0034-gpucomputation/) have separate runtime contracts.

## [Any](any/)

An explicitly dynamic value that must be type-matched before concrete operations.

## [Bool](bool/)

A boolean type that can be either true or false. Used for logical operations and conditionals.

## Float

A 64-bit IEEE-754 floating-point number. Arithmetic can produce infinity, NaN, subnormal values and signed zero. Division and remainder by zero use an explicit arithmetic handler. See [floating-point results](/spec/0037-arithmeticeffects/#floating-point-results--float-ieee-results).

## [HttpResponse](httpresponse/)

A built-in type representing an HTTP response with status code, headers, content type, body, and streaming capabilities. Used by HTTP server handlers to return structured responses to clients.

## [Int](int/)

A 64-bit signed integer type. Can represent whole numbers from -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807.

## [Process handle (int)](processhandle/)

`spawnProcess` returns an integer process ID inside Result. Use it to wait for completion and release runtime resources; the callback receives output and exit events.

## [String](string/)

A sequence of characters representing text. Supports string interpolation and escape sequences.
