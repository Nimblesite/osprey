# Clean Architecture todo port: Osprey makes MediatR redundant

## The concept

Cross-cutting concerns are the logging, validation, authorization, timing, error handling, retries and transactions that wrap every request in a business app. In C#, the usual answer is [MediatR](https://github.com/jbogard/MediatR): a library on top of dependency injection and reflection, where each concern is an `IPipelineBehavior<TRequest, TResponse>` class that receives a once-only `next()` delegate. Osprey answers the same problem with a language feature: algebraic effects, their handlers and multi-shot continuations.

This example ports a real, widely used C# app and proves the claim in code. It lives in `src/osprey/examples/cleanarch_todo/`.

## Source and licence — [EXAMPLE-CLEANARCH-SOURCE]

- **Source:** [jasontaylordev/CleanArchitecture](https://github.com/jasontaylordev/CleanArchitecture) at commit `9e1042be109b6b73488695bcd6840c8ea78db5f0`. It is the Clean Architecture solution template with ASP.NET Core Minimal API endpoints, MediatR behaviours and FluentValidation.
- **Licence:** MIT, "Copyright (c) 2023 JasonTaylorDev". `NOTICE.md` in the example names the repository, the URL and the commit, and reproduces the full MIT text. Each ported file names the C# file it comes from. C# excerpts in the README are short, attributed and quoted for comparison.

## Invariants

These hold for the life of the example. A change that breaks one is a defect, and fixing it is in scope.

### [EXAMPLE-CLEANARCH-INV-REDUNDANT] MediatR is redundant

The port uses no mediator library, framework or DI container. Effects, handlers and continuations are the mediator and the pipeline. The dispatcher is a few plain functions. Every MediatR concept maps to the Osprey construct that replaces it, and the README shows each mapping:

- `IRequest` / `IRequestHandler`
- `IPipelineBehavior` and its `RequestHandlerDelegate next`
- `INotification` / `INotificationHandler`
- `AddMediatR`, assembly scanning and the open-generic registration order

### [EXAMPLE-CLEANARCH-INV-MULTISHOT] Multi-shot continuations are the centrepiece

At least four real uses each resume the **same** captured request continuation more than once:

- **[EXAMPLE-CLEANARCH-MULTISHOT-RETRY]:** a transient store conflict resumes the request again from a fresh snapshot, and the abandoned branch's writes roll back.
- **[EXAMPLE-CLEANARCH-MULTISHOT-AUTHZ]:** one request runs as anonymous, user and administrator, and reports who may run it and what each gets.
- **[EXAMPLE-CLEANARCH-MULTISHOT-VALIDATE]:** each candidate fix is explored by resuming once per candidate, and every validation failure is still collected.
- **[EXAMPLE-CLEANARCH-MULTISHOT-PLAN]:** one captured query is resumed with several page sizes and numbers to plan pagination.

Multiplicity is declared ([MULTI-DECL]) and handler obligations are explicit ([MULTI-HANDLE]). Replay is safe ([MULTI-REPLAY]): effects from an abandoned branch never leak. A port that uses only one-shot handlers fails this invariant.

### [EXAMPLE-CLEANARCH-INV-NO-SKIP] Multi-shot is never skipped

No agent may skip, stub, defer, simulate or downgrade any multi-shot use in [EXAMPLE-CLEANARCH-INV-MULTISHOT], **even if the compiler does not support it yet**. That rules out:

- one-shot handlers standing in for multi-shot ones;
- loops or re-dispatching that imitate a resume;
- commented-out code, "TODO" markers or feature flags;
- removing or weakening the uses.

Skipping is an egregious violation.

- The tests **assert multi-shot behaviour** directly: resume counts, each branch's distinct result, rolled-back writes and unpublished events from abandoned branches.
- A test that fails because the compiler's multi-shot support is unfinished is **correct**. It stays red ([COMPILER-RED-FAILS], "a red test is correct"). It is never weakened, skipped or deleted to make the suite pass.
- Each such failure is a compiler gap. It is reported with the exact compiler or runtime message, and fixing the compiler is the follow-up, never changing the example.

### [EXAMPLE-CLEANARCH-INV-ELEGANT] More elegant than the original

Each Osprey behaviour is shorter than its C# behaviour class plus its registration. The port has no marker interfaces, generic pipeline ceremony, `next` delegate, reflection or registration order to get wrong. Each handler is a small pure function. The README backs this with a line-count table and short side-by-side excerpts.

### [EXAMPLE-CLEANARCH-INV-BETTER] Why Osprey over C#

The README argues, with code from this example, that a language feature beats a bolt-on framework:

- An unhandled concern is a compile error from the effect rows. In C#, a missing registration fails at runtime.
- Handler order is visible lexical scope.
- Multi-shot replay is impossible with a once-only `next()`.
- Immutable records and `Result` replace exceptions.
- Nothing uses reflection.

Every claim must be true of the code. A claim the code does not demonstrate is a defect.

### [EXAMPLE-CLEANARCH-INV-BOOK] Clean Architecture as the book defines it

The example follows Robert C. Martin's *Clean Architecture* (2017). It does not follow the common C# template convention of Domain/Application/Infrastructure/Web projects with mapping and layering ceremony. In particular:

- **[EXAMPLE-CLEANARCH-BOOK-DEPENDENCY] The Dependency Rule.** Source dependencies point only inward. Entities and use cases never import HTTP, JSON, storage, clocks, logging or any framework module.
- **[EXAMPLE-CLEANARCH-BOOK-BOUNDARY] Inner circles own their boundaries.** A use case declares the effects it needs: `Store`, `Clock`, `Publish`, `Auth` and `Log`. Outer-ring handlers implement them. This is dependency inversion as a language feature, with no interfaces, adapters or container.
- **[EXAMPLE-CLEANARCH-BOOK-SCREAMING] Screaming architecture.** Modules are named for the domain (todo lists, todo items) and its use cases, not for technical layers. The code has no `Services`, `Repositories`, `Dtos` or `Mappers` modules.
- **[EXAMPLE-CLEANARCH-BOOK-DATA] Plain data crosses boundaries.** Immutable records go in and come out of use cases. Nothing maps per layer (no AutoMapper-style projections or DTO copies of every entity). JSON is decoded into those records once, at the outermost edge.
- **[EXAMPLE-CLEANARCH-BOOK-DETAILS] The database, the web and the framework are details.** Swapping the in-memory store handler or the HTTP edge never touches an entity or a use case, and the tests prove it by running every use case without HTTP.

- **[EXAMPLE-CLEANARCH-BOOK-QUOTE] Quote the source.** Where the README or a module comment states a rule above, it quotes Robert C. Martin's own wording from [The Clean Architecture](https://blog.cleancoder.com/uncle-bob/2012/08/13/the-clean-architecture.html) (The Clean Code Blog, 13 August 2012). Each quote is exact, short (under 15 words), in quotation marks and attributed with the link. Quotes are few, and never a long excerpt or a reconstruction of the post. Everything else is in our own words.

### [EXAMPLE-CLEANARCH-INV-FLAVOR] Default flavour only

Every source and test file is Default-flavour `.osp`. The example has no `.ospml` files.

### [EXAMPLE-CLEANARCH-INV-FAITHFUL] Same behaviour as the C# app

- The domain, commands and queries match the C# app, and so do the validators' rules: title required, at most 200 characters, unique list titles, the supported colours, and pagination edges.
- Routes, verbs and status codes match `src/Web/Endpoints`.
- JSON appears only at the edge, decoded into typed records in one boundary module.
- Osprey never panics, aborts or exits ([FAILURE-EXPLICIT]).

### [EXAMPLE-CLEANARCH-INV-TESTS] Tests prove the claims

End-to-end use-case tests cover many permutations under GC and ARC. They assert:

- how many times each continuation resumed, and each branch's distinct result;
- that rolled-back writes are absent;
- that domain events from abandoned branches are not published;
- that all validation failures are collected.

The suite is not in the CI pipeline yet. Adding it is a later, explicit step.

## Status

- [ ] Port written by a pi agent under the lead's review (`zai/glm-5.3`, 2026-11-22)
- [ ] Every invariant above checked by the lead against the code and the tests
- [ ] Committed. Tests not yet wired into `make` or CI.
