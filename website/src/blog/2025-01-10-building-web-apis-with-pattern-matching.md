---
layout: page.njk
title: "Modeling Web API Results with Pattern Matching"
excerpt: "Use union variants to represent API outcomes and match each case explicitly."
description: "Model web API outcomes with Osprey union types and pattern matching."
tags: ["blog", "web-development", "pattern-matching", "type-safety", "apis"]
author: "Christian Findlay"
readingTime: 3
image: /assets/images/blog/building-web-apis-with-pattern-matching.png
---

An API operation can return a union that lists its domain outcomes:

```osprey
type CreateUserResult =
    Created { id: int }
    | ValidationFailed { message: string }
    | DuplicateEmail { email: string }
    | DatabaseFailure { message: string }
```

A handler maps each outcome to an HTTP response:

```osprey
fn json(status: int, body: string) -> HttpResponse = HttpResponse {
    status: status,
    headers: "Content-Type: application/json",
    contentType: "application/json",
    streamFd: -1,
    isComplete: true,
    partialBody: body
}

fn toResponse(result: CreateUserResult) -> HttpResponse = match result {
    Created { id } =>
        json(201, "{\"id\": ${toString(id)}}")
    ValidationFailed { message } =>
        json(400, "{\"error\": \"${message}\"}")
    DuplicateEmail { email } =>
        json(409, "{\"error\": \"${email} is already registered\"}")
    DatabaseFailure { message } =>
        json(502, "{\"error\": \"database failure\"}")
}
```

For supported patterns, the checker reports a non-exhaustive match when a
variant is omitted. Adding a variant therefore identifies matches that need a
new policy.

This does not prove that an API cannot fail: HTTP, parsing, FFI code and the
runtime still have failure boundaries. It makes the modeled domain outcomes
visible in ordinary data and keeps their HTTP mapping in one function.

See the [HTTP specification](/spec/0014-http/) and [pattern-matching
specification](/spec/0007-patternmatching/) for the implemented contracts.
