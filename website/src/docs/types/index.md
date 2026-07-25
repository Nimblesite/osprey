---
layout: page
title: "Built-in Types"
description: "Complete reference for all built-in types in Osprey"
---

All built-in types available in Osprey.

## [Any](any/)

An explicitly dynamic value that must be type-matched before concrete operations.

## [Bool](bool/)

A boolean type that can be either true or false. Used for logical operations and conditionals.

## [HttpResponse](httpresponse/)

A built-in type representing an HTTP response with status code, headers, content type, body, and streaming capabilities. Used by HTTP server handlers to return structured responses to clients.

## [Int](int/)

A 64-bit signed integer type. Can represent whole numbers from -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807.

## [ProcessHandle](processhandle/)

A handle to a spawned async process. Contains the process ID and allows waiting for completion and cleanup. Process output is delivered via callbacks registered with the runtime.

## [String](string/)

A sequence of characters representing text. Supports string interpolation and escape sequences.
