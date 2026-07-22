---
layout: page.njk
title: "Building Type-Safe Web APIs with Osprey's Pattern Matching"
excerpt: "Discover how Osprey's exhaustive pattern matching and algebraic data types eliminate entire classes of runtime errors in web API development, making your services more reliable and maintainable."
description: "Learn how Osprey’s exhaustive pattern matching and algebraic data types make web API states explicit, checked, and easier to maintain as services evolve."
date: 2025-01-10
tags: ["blog", "web-development", "pattern-matching", "type-safety", "apis"]
author: "Christian Findlay"
readingTime: 6
image: /assets/images/blog/building-web-apis-with-pattern-matching.png
---

Web APIs are the backbone of modern applications, but they're also where things go wrong most often. Null reference exceptions, unhandled error cases, and missing validation logic plague even the most carefully written services. **What if your programming language could eliminate these problems entirely?**

Osprey's **exhaustive pattern matching** and **algebraic data types** provide exactly that guarantee. Let's explore how to build rock-solid web APIs that handle every possible case—and how the compiler ensures you never forget an edge case.

## The Problem with Traditional Error Handling

Most web frameworks handle errors through exceptions or error codes. Here's typical Node.js/Express code:

```javascript
app.post('/users', async (req, res) => {
  try {
    const user = await createUser(req.body);
    if (user) {
      res.json({ success: true, data: user });
    } else {
      res.status(400).json({ error: "Failed to create user" });
    }
  } catch (error) {
    if (error.code === 'DUPLICATE_EMAIL') {
      res.status(409).json({ error: "Email already exists" });
    } else {
      res.status(500).json({ error: "Internal server error" });
    }
  }
});
```

**What's wrong here?** The compiler can't verify you've handled every error type. You might forget to handle a database timeout, miss a validation error, or incorrectly assume what `createUser` returns.

## Modeling API Responses with Algebraic Data Types

Osprey takes a different approach. We model all possible outcomes as a single **algebraic data type** — a union whose variants each carry their own record of fields:

```osprey
type CreateUserResult =
    Created            { user: User, id: UserId }
    | ValidationFailed { fields: List<string>, messages: List<string> }
    | DuplicateEmail   { email: string }
    | DatabaseTimeout  { retryAfter: int }
    | DatabaseFailure  { message: string }
    | InternalFailure  { context: string }
```

Now our API handler **must** handle every case. A small helper keeps each arm to a single line — `HttpResponse` is Osprey's built-in response record:

```osprey
fn json(status: int, body: string) -> HttpResponse = HttpResponse {
    status: status,
    headers: "Content-Type: application/json",
    contentType: "application/json",
    streamFd: -1,
    isComplete: true,
    partialBody: body
}

fn createUserHandler(request: HttpRequest) -> HttpResponse =
    match createUser(parseUserData(request.body)) {
        Created { user, id } =>
            json(201, "{\"id\": ${toString(id)}}")
        ValidationFailed { fields, messages } =>
            json(400, "{\"error\": \"validation failed\"}")
        DuplicateEmail { email } =>
            json(409, "{\"error\": \"${email} is already registered\"}")
        DatabaseTimeout { retryAfter } =>
            json(503, "{\"error\": \"retry after ${toString(retryAfter)}s\"}")
        DatabaseFailure { message } =>
            json(502, "{\"error\": \"database error\"}")
        InternalFailure { context } =>
            json(500, "{\"error\": \"internal error in ${context}\"}")
    }
```

**The compiler guarantees** you handle every case. If you add a new variant to `CreateUserResult`, every `match` that handles it will fail to compile until you add the new arm.

## Building a Complete User API

Let's build out a user management API to see how this scales. Osprey has no `module` keyword yet — types and functions live at the top level. Each operation returns a domain union, so the caller is forced to handle every outcome:

```osprey
type User = { id: int, email: string, name: string, isActive: bool }

type UserFilter = All | Active | Inactive | EmailDomain { domain: string }

type UserQuery =
    Found       { user: User }
    | Missing     { id: int }
    | Forbidden   { action: string }
    | QueryFailed { details: string }

// GET /users/:id — permission gating is just another match arm.
// Osprey is expression-based, so there is no early `return`.
fn getUser(id: int, auth: AuthToken) -> UserQuery =
    match hasPermission(auth, "users:read") {
        false => Forbidden { action: "read user" }
        true  => lookupUser(id)
    }
```

Validation and lookup compose as nested matches. The built-in `Result<T, E>` type (whose variants are `Success` and `Error`) threads naturally into our domain union:

```osprey
// PUT /users/:id
fn updateUser(id: int, updates: UserUpdates, auth: AuthToken) -> UserQuery =
    match hasPermission(auth, "users:write") {
        false => Forbidden { action: "update user" }
        true  => match validate(updates) {
            Error   { message } => QueryFailed { details: message }
            Success { value }   => applyUpdate(id, value)
        }
    }
```

## HTTP Response Conversion

Converting our domain types to HTTP responses becomes a pure mapping function that reuses the `json` helper from earlier:

```osprey
fn toHttpResponse(q: UserQuery) -> HttpResponse = match q {
    Found { user }          => json(200, "{\"email\": \"${user.email}\"}")
    Missing { id }          => json(404, "{\"error\": \"user ${toString(id)} not found\"}")
    Forbidden { action }    => json(403, "{\"error\": \"permission denied: ${action}\"}")
    QueryFailed { details } => json(500, "{\"error\": \"internal error\"}")
}
```

## Request Routing with Pattern Matching

Osprey matches a single value per `match`, so multi-key routing is expressed as **nested matches** rather than tuple patterns:

```osprey
fn routeRequest(method: string, path: string, auth: AuthToken) -> HttpResponse =
    match method {
        "GET" => match path {
            "/health" => json(200, "{\"status\": \"ok\"}")
            "/users"  => toHttpResponse(getUser(1, auth))
            _         => json(404, "{\"error\": \"not found\"}")
        }
        "POST" => json(201, "{\"message\": \"created\"}")
        _      => json(405, "{\"error\": \"method not allowed\"}")
    }
```

## Middleware as Function Composition

Functions are first-class, so middleware is just a function that wraps a handler. The `next` parameter is the handler to run if the gate passes — again, no early return, the gate is simply a `match`:

```osprey
fn withAuth(
    auth: AuthToken,
    action: string,
    next: fn(AuthToken) -> UserQuery
) -> UserQuery =
    match hasPermission(auth, action) {
        false => Forbidden { action: action }
        true  => next(auth)
    }
```

Because handlers are ordinary values, you compose cross-cutting concerns the same way you compose any other function — with the pipe operator `|>`.

## Testing Becomes Trivial

Since every handler is a pure function over data, testing is incredibly straightforward. Osprey programs are exercised with the **differential harness**: a program's `stdout` is byte-compared against a checked-in `.expectedoutput` file. A test is just a program that prints each outcome:

```osprey
fn main() -> int {
    print(describe(getUser(1, adminAuth)))   // Found ...
    print(describe(getUser(2, adminAuth)))   // Forbidden ...
    print(describe(getUser(9, adminAuth)))   // Missing ...
    0
}
```

Because the result of each call is plain data, the expected output is completely deterministic — there is no hidden state to mock.

## The Reliability Advantage

This approach eliminates entire categories of production bugs:

- **No null reference exceptions** - Absence is modelled explicitly with union variants
- **No unhandled error cases** - Pattern matching forces you to handle every scenario  
- **No silent failures** - Every operation's outcome is explicitly modeled
- **No incorrect status codes** - HTTP responses are generated deterministically
- **No missing validation** - Validation is built into the type system

## Performance Benefits

Despite the high-level abstractions, Osprey compiles to efficient code. Functional pipelines over ranges are fused into a single loop:

```osprey
fn add(a: int, b: int) -> int = a + b

// This high-level code...
let result = range(1, 1000)
    |> filter(fn(n: int) => (n % 2) == 0)
    |> map(fn(n: int) => n * n)
    |> fold(0, add)

// ...compiles to a single loop with no intermediate lists (stream fusion).
```

The pattern matching compiles to efficient jump tables, and the functional pipelines are optimized away entirely.

## Conclusion

Building web APIs with Osprey's pattern matching transforms error-prone, defensive programming into **compiler-verified correctness**. You can't ship a handler that forgets to handle an error case because **it won't compile**.

This isn't just academic elegance—it's practical reliability. When your API serves millions of requests per day, the difference between "it should work" and "it can't fail" is the difference between 3 AM pages and peaceful sleep.

The functional programming revolution isn't coming—**it's here**. And for web development, the benefits are too compelling to ignore.

---

*Ready to try building your own type-safe APIs? Browse the [documentation](/docs/) or experiment in the [playground](/playground/).*

> **Editor's note (updated 2026-06-20):** This post was first published in January 2025, early in Osprey's development. Osprey's syntax has evolved considerably since then — among other changes, the language settled on `=>` for match arms, record-style union variants, expression-bodied functions, lowercase primitive types (`int`/`string`/`bool`), and `Result` with `Success`/`Error`. The code samples above have been revised to match the current language. The original article is preserved unchanged in this site's Git history — browse the [repository](https://github.com/Nimblesite/osprey) and view this file's history to read it exactly as first published.
