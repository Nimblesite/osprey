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
    | DatabaseUnavailable
```

One function maps each outcome to an HTTP response:

```osprey
fn json(status, body) = HttpResponse {
    status: status,
    headers: "Content-Type: application/json",
    contentType: "application/json",
    streamFd: -1,
    isComplete: true,
    partialBody: body
}

fn toResponse(result) = match result {
    Created { id } => json(201, "{\"id\": ${id}}")
    ValidationFailed { message } => json(400, "{\"error\": \"${message}\"}")
    DuplicateEmail { email } => json(409, "{\"error\": \"${email} is already registered\"}")
    DatabaseUnavailable => json(503, "{\"error\": \"try again later\"}")
}
```

The checker rejects a match that omits a variant, so adding a variant points at every match that needs a new case.

HTTP, parsing and FFI code can still fail; this keeps the modeled outcomes in ordinary data and their HTTP mapping in one function.

See the [HTTP specification](/spec/0014-http/) and [pattern-matching
specification](/spec/0007-patternmatching/) for the implemented contracts.
