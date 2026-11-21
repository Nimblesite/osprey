# Clean Architecture todo: MediatR is redundant

Logging, validation, authorization, timing, error handling and transactions wrap every request in a business app. They are called cross-cutting concerns. In C#, the usual answer is [MediatR](https://github.com/jbogard/MediatR): a library where each concern is a pipeline behaviour class that receives a `next()` delegate it can call once.

Osprey answers the same problem with a language feature. A use case asks for what it needs with an **effect**, a **handler** decides what happens, and the rest of the request is a **continuation** the handler can run. A multi-shot continuation can run more than once, so one handler can retry a request, or answer the same request for three different users.

This example is a port of the todo app in [jasontaylordev/CleanArchitecture](https://github.com/jasontaylordev/CleanArchitecture) (MIT, see [NOTICE.md](NOTICE.md)). It has the same requests, validation messages, routes and status codes, in half the code, and it does four things the C# cannot.

## Run it

From the repository root:

```bash
osprey src/osprey/examples/cleanarch_todo --run
```

Expected: `cleanarch-todo API on 127.0.0.1:8080`. Then, in another terminal:

```bash
curl -i -X POST http://127.0.0.1:8080/api/TodoLists -d '{"title":"Shopping","colour":""}'
```

Expected: `HTTP/1.1 201 Created` with `Location: /api/TodoLists/1`.

Run the tests under each memory backend:

```bash
osprey test src/osprey/examples/cleanarch_todo/test --memory=gc
```

```bash
osprey test src/osprey/examples/cleanarch_todo/test --memory=arc
```

Expected from each: `# tests=34 passed=34 failed=0 skipped=0`.

## MediatR is redundant

Every MediatR concept has an Osprey construct that replaces it. None of them is a library.

| MediatR | Osprey | Where |
| --- | --- | --- |
| `IRequest<T>`, one class per request | One `Request` union | [todo_request.osp](src/todo_request.osp) |
| `IRequestHandler<,>`, found by reflection | One `match` over the union | `run` in [todo_dispatch.osp](src/todo_dispatch.osp) |
| `IPipelineBehavior<,>` and `next()` | A line of `send`; the lines after it are `next()` | `send` in [todo_dispatch.osp](src/todo_dispatch.osp) |
| `INotification`, `INotificationHandler<>` | The `Events` effect and its handler | [todo_host.osp](src/todo_host.osp) |
| `AddMediatR`, assembly scanning, registration order | Nothing: the order is the order of the lines | `send` |
| `IApplicationDbContext`, `IUser`, `ILogger<T>`, `TimeProvider` | The `Store`, `Auth`, `Log` and `Clock` effects | [todo_effects.osp](src/todo_effects.osp) |

The whole pipeline is one function. Each behaviour is a line, in the order it runs:

```osprey
export fn send(request) = {
    let principal = perform Auth.who()
    perform Log.info("CleanArchitecture Request: ${TodoRequest::requestName(request)} by ${principal.name}")
    let startedAt = perform Clock.nowMs()
    let outcome = TodoReplay::transacted(fn() => attempt(request, principal))
    logUnhandled(request, outcome)
    warnWhenSlow(request, startedAt)
    outcome
}
```

## Multi-shot continuations

A handler arm receives the rest of the request as `resume`. Declaring an operation `control many` lets the arm call `resume` more than once. Each call runs its own branch from the same point, and nothing before that point runs again.

MediatR's `next()` runs once, so none of the four uses below can be written as a pipeline behaviour.

### Retry the same request after a lost commit

`[EXAMPLE-CLEANARCH-MULTISHOT-RETRY]` When a rival writer commits first, the transaction handler resumes the same request again, up to three attempts. The lost attempt's writes are rolled back and its events are unpublished before the next attempt starts.

```osprey
handle Tx {
    begin => {
        let mark = perform Events.mark()
        let first = resume(1)
        let second = match lostCommit(first) {
            true => resume(rewound(mark, 2))
            false => first
        }
        ...
    }
}
```

### Ask who may run a request

`[EXAMPLE-CLEANARCH-MULTISHOT-AUTHZ]` `TodoProbe::whoMayRun(request)` answers the one question "who is signed in?" three times: as nobody, as a user and as the administrator. One sent request gives three answers, and each branch's writes are undone before the next.

```osprey
handle Auth {
    who => {
        let base = perform Store.dump()
        let mark = perform Events.mark()
        listConcat(undone(base, mark, resume(anonymous())), listConcat(
            undone(base, mark, resume(member())),
            undone(base, mark, resume(administrator()))))
    }
}
```

For the purge request the three answers are unauthorized, forbidden and "purged 2 lists", and both lists are still there afterwards.

### Try each fix for an invalid request

`[EXAMPLE-CLEANARCH-MULTISHOT-VALIDATE]` `TodoProbe::whichFixesWork(request)` resumes validation once per candidate: the request as sent, its title trimmed, its title cut to 200 characters, and its colour reset. Every branch still collects all of its own failures.

### Plan pagination from one query

`[EXAMPLE-CLEANARCH-MULTISHOT-PLAN]` `TodoProbe::pagePlan(listId)` resumes one captured query with four page shapes and returns all four pages.

### What the tests assert

[multishot_cases.osp](test/cases/multishot_cases.osp) checks each use directly:

- **How often the request resumed.** The fixture clock advances one per reading, so the reading count shows how much of the request each resumption re-ran.
- **Each branch's own answer**, in order.
- **No write survives an abandoned branch.** The store holds the same rows afterwards, and a branch that created a list got the same id as the branch before it.
- **No event survives an abandoned branch.** A retried completion publishes its event once; three lost attempts publish nothing.

## Why Osprey over C#

Each claim below is true of this code.

- **A missing handler is a compile error.** Remove `TodoHost::withStore` from [main.osp](src/main.osp) and the build stops with an unhandled `Store` operation. In C#, a missing registration fails when the request arrives.
- **Handler order is the order of the lines.** `send` runs its behaviours top to bottom. In C#, the order is the order of `AddOpenBehavior` calls in another file.
- **A request can be resumed more than once.** The four uses above need it. A `next()` delegate cannot do it.
- **Failures are values.** Every failure is a `Fault` in a `Result`; the `Fail` handler in `attempt` is the only place a request ends early. Nothing throws.
- **Nothing uses reflection.** The request catalogue, the authorization rules and the handler binding are plain data and one `match`.

## Clean Architecture as the book defines it

The example follows Robert C. Martin's rules, not the project layout the C# template uses.

- **The Dependency Rule.** In Martin's words, "source code dependencies can only point inwards" ([The Clean Architecture](https://blog.cleancoder.com/uncle-bob/2012/08/13/the-clean-architecture.html), The Clean Code Blog, 13 August 2012). [todo_domain.osp](src/todo_domain.osp) imports nothing. The use cases import the domain and name effects; they never import JSON, HTTP or a store.
- **Inner circles own their boundaries.** A use case declares the operations it needs. The outer ring answers them in [todo_host.osp](src/todo_host.osp). This is dependency inversion with no interface, adapter or container.
- **Screaming architecture.** The modules are named for todo lists, todo items and their use cases. There is no `Services`, `Repositories` or `Mappers` module.
- **Plain data crosses boundaries.** The post says the data that crosses a boundary should be simple structures. Immutable records go into and out of every use case, and nothing is copied per layer.
- **The database and the web are details.** The post calls both details. The tests run every use case with no web server, and [edge_todo.osp](src/edge_todo.osp) is the only file that knows JSON.

## Half the code

The port is half the size of the C# it reproduces, counted two ways.

| | Files | Lines | Code lines |
| --- | --- | --- | --- |
| C# files the port reproduces | 54 | 1,675 | 1,252 |
| Osprey port | 10 | 836 | 624 |
| Osprey only: retry and rollback, [todo_replay.osp](src/todo_replay.osp) | 1 | 124 | 93 |
| Osprey only: what-if probes, [todo_probe.osp](src/todo_probe.osp) | 1 | 119 | 80 |

A code line is a line that is neither blank nor a comment. The C# count takes `src/Domain`, `src/Application`, `src/Web/Endpoints`, `src/Web/Infrastructure`, `src/Web/Services`, `src/Web/Program.cs`, the `DependencyInjection.cs` files, `ApplicationDbContext.cs`, the entity configurations and `DispatchDomainEventsInterceptor.cs` at the pinned commit. It leaves out what the port does not reproduce: the weather forecast and user endpoints, the three OpenAPI transformers, audit stamps (`AuditableEntityInterceptor.cs`, `BaseAuditableEntity.cs`) and `Result.cs`.

The pipeline itself shrinks the most:

| C# file | C# lines | Osprey lines | Osprey |
| --- | --- | --- | --- |
| LoggingBehaviour.cs | 35 | 1 | One line of `send` |
| UnhandledExceptionBehaviour.cs | 30 | 11 | The `Fail` handler and `logUnhandled` |
| AuthorizationBehaviour.cs | 80 | 17 | `authorize`, `allows`, `denial`, `inAnyRole` |
| ValidationBehaviour.cs | 34 | 8 | `validated` |
| PerformanceBehaviour.cs | 54 | 9 | Two lines of `send` and `warnWhenSlow` |
| DependencyInjection.cs | 25 | 0 | The order of the lines in `send` |

## Compiler gaps

Three limits shaped this code. Each is a compile error with a clear message, never a wrong answer.

- **A multi-shot continuation cannot cross a built-in's callback.** `authorize` filters the rules first and raises one failure afterwards, because a request made inside `forEachList`'s callback cannot be captured. The handlers a request needs are installed per request for the same reason: a continuation never leaves the HTTP callback that made it.
- **A function stored in a value cannot make a captured request.** The probes pass a `TitleFix` value where a function would have been natural.
- **An empty list bound with `mut` needs its type.** `mut buffer: List<TodoItemCompleted> = []` in [todo_host.osp](src/todo_host.osp) carries the one annotation in the example that inference should have made unnecessary.

Not ported from the C#: audit stamps, OpenAPI documents, and the weather forecast and user endpoints.
