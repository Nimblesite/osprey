# Appendices and next steps

**Working reference.** The command and qualification tables have been audited against the current development compiler and repository contracts. The syntax sheet and longer agent templates remain outlines. The edition still needs a publication compiler pin; consult `book.json` and `evidence.json` for its recorded build identity and checks.

## Appendix A — Command quick reference

These commands assume `osprey` is on your executable search path and the named source is in the current directory. In the repository, use `target/release/osprey` from its root. The hello source is the Chapter 1 example; `title.test.osp` is the Chapter 8 test.

| Job | Command |
|---|---|
| Check a source | `osprey hello.osp --check` |
| Compile and run | `osprey hello.osp --run` |
| Create a native executable | `osprey hello.osp --compile -o hello` |
| Run a test file | `osprey test title.test.osp` |
| Preview formatting without editing | `osprey fmt --stdout hello.osp` |
| Check a portable target | `osprey hello.osp --check --target=wasm32` |
| Select a native memory backend | `osprey hello.osp --run --memory=arc` |
| Build native debug code | `osprey hello.osp --compile --debug -o hello-debug` |
| Run with native profiling | `osprey hello.osp --run --profile` |

For a multi-file project, supply its directory or `osprey.toml` in place of the source path. `osprey build project-directory` is the project build form. Discovery, entry selection, and flavor overrides follow [Modules and Namespaces](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0025-ModulesAndNamespaces.md).

`--check` also enforces target capability restrictions. WebAssembly and mobile accept only `--memory=default`; native debug and profiler flags are not portable target options. Mobile compilation emits a static archive and C header and cannot be executed by `--run` without a platform host. Profiling produces additional output files; use it on a real workload and keep its output directory intentional.

The formatting command keeps the selected source flavor. It does not translate a `.osp` program into `.ospml`. Never replace a platform rejection with a different target flag and claim the original target was tested.

## Appendix B — Default syntax on one page

Planned scan sheet for bindings, functions, calls, blocks, records, unions, matching, lists, maps, pipelines, Results, effects, and fibers. It is a memory aid, not a second specification. Until it is completed, the examples in Chapters 1–3 and 8–10 are the checked teaching reference.

The effects entries must use `effect`, `perform`, callable `handler` values, and rest-of-block `handle`. `h(work)` installs a handler before invoking its zero-argument function. Value arms return to the request; declared `control` operations have the separate continuation behavior discussed in Chapter 9. Removed `handle ... in` and `handle ... do` forms must not reappear in the sheet.

## Appendix C — Agent prompt and verification recipe

Planned templates for a small feature, a behavior-preserving refactor, and an optional flavor translation. Every template names invariants and ends with compiler or test evidence. The completed chapters already include bounded handoffs that can be used now.

Require the agent to distinguish what was checked from what was executed. For an effect boundary, name the successful result, expected failures, and handler selected by each test. For a flavor translation, preserve flat versus curried function shape and compare outputs. For platform work, report each host build and execution separately; shared tests do not substitute for an unrun simulator test.

## Appendix D — Current qualifications

The following qualifications describe the current implementation. A future design specification does not change this table until implementation and executable evidence support the claim.

| Area | Current boundary |
|---|---|
| Effects | Callable value handlers and static discharge have native, WebAssembly, and mobile paths. Dynamic control handlers are native-only. Independently quantified effect rows, reusable or escaping continuations, and the wider staged design remain delivery work. |
| Concurrency | Normal native fibers use one pthread each. Channels have a positive-capacity typed buffer. WebAssembly excludes this runtime; lexical scopes, cancellation, deadlines, and races are planned. |
| Ownership | Managed immutable values may be co-owned across fiber threads. Message passing does not promise physically separate heaps or a deep copy of every value. |
| Native memory | `default`, `gc`, and `arc` are build options. Default retains general allocations. Conservative GC is supported on Apple and glibc-Linux, is not supported on Windows, and disables collection after another allocator thread appears. |
| Portable/mobile memory | WebAssembly, iOS, and Android currently accept only `default`. Mobile returned strings have no stable host release API. Strict static-memory checking is not implemented. |
| WebAssembly | Emits a WASI command module; browser execution needs the supported host bridge. Files depend on host access. Native HTTP/WebSocket, process, terminal, fiber, and arbitrary-FFI facilities are unavailable. |
| Mobile | iOS device/simulator and Android ARM64/x86-64 targets emit archives and headers for native hosts. The host supplies UI, networking, lifecycle, and asynchronous completion events. |
| Modules and packages | Project discovery, namespaces, modules, and cross-flavor imports work. The package manager and registry remain design work. |
| Flavors | Default and ML are currently available. One source selects one flavor; a project may mix them. Formatting does not convert flavors. |
| Optimization and GPU | Tail-call optimization is not implemented. Current GPU kernel execution uses host loops; device execution remains planned. |
| Foreign resources | C integration crosses Osprey's memory-safety boundary. Files, response handles, sockets, and other external resources still need their explicit cleanup APIs. |

The narrow authorities are the [effects delivery plan](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0016-algebraic-effects-and-handlers.md), [fiber contract](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0011-LightweightFibersAndConcurrency.md), [memory contract](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0018-MemoryManagement.md), and the target specifications linked in Chapter 12. Package, structured-concurrency, and GPU delivery status lives in plans 0020, 0026, and 0023 respectively.

## Appendix E — Flight Log source map

The current checked source lives under `Book/examples/`:

| Chapter | Source directory | Checkpoint evidence |
|---|---|---|
| 1 | `chapter-01/` | First output and the optional ML twin. |
| 2 | `chapter-02/` | Named values and the Flight Log summary. |
| 3 | `chapter-03/` | Inference, a record, and a stored type rejection. |
| 8 | `chapter-08/` | Executed assertions and a Flight Log test suite. |
| 9 | `chapter-09/` | Handler choice, scope, static selection, control behavior, and required-handler/type errors. |
| 10 | `chapter-10/` | Scripted failures, actual file persistence, and an optional local HTTP adapter. |

Run `make check-examples` from `Book` to check the completed examples against their outputs and stored rejection diagnostics. The Chapter 10 file program runs in a temporary directory. Its optional native HTTP source has separate fixture-server instructions in `examples/chapter-10/README.md`. The remaining chapters are outlines; they do not yet have completed Flight Log checkpoints.

## Continue learning

Use the live [Osprey documentation](https://www.ospreylang.dev/docs/), [specifications](https://github.com/Nimblesite/osprey/tree/main/docs/specs), [status page](https://www.ospreylang.dev/status/), and [Playground](https://www.ospreylang.dev/playground/) for behavior beyond this edition. Confirm alpha-era changes against the current compiler and tests before updating book claims.
