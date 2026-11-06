---
mlTwins: manual
layout: page
title: Algebraic effects
description: Effects abstract side effects. Functions say what they need; the caller picks the handler that does it, so tests swap the disk, network or database for a test double.
permalink: /docs/effects/
tags: [algebraic-effects, handlers, applications, testing]
---

{% from "native-capture.njk" import capture %}

**Algebraic effects let a function ask for something it needs, such as a file, a web request or a log line, without knowing who will provide it.** Three words do the work:

- An **effect** is a named list of requests, such as `Files.read` and `Files.write`. It declares what can be asked for and the types involved, and contains no code.
- **`perform`** makes a request: `perform Files.read("osprey.toml")`. The function that performs it does not take a disk as a parameter and does not know which disk it will get.
- A **handler** is the code that answers those requests. One `Files` handler reads and writes the real disk. Another keeps files in memory for a test. Another pretends the disk is full.

**Installing a handler** means running some code under it. Every request made inside that code goes to that handler, however many function calls deep the request is. So you install handlers once, at the top: the program's entry point installs the real disk, and each test installs the fake it needs. Nothing in between passes a disk, a logger or a database connection around, and the same application code runs in production and in tests without a mocking framework. If code performs a request and no handler is installed above it, the program does not compile.

## A real example: the package manager's file system

The Osprey package manager does all its file work through the `Files` effect in [`packages/manager/host/src/host.osp`](https://github.com/Nimblesite/osprey/blob/main/src/osprey/packages/manager/host/src/host.osp). The program below trims that effect to two operations and uses it for one job from the manager's install transaction: update the manifest, write the lock file, and put the old manifest back if the lock write fails.

```osprey
// The file-system effect from the package manager's host boundary
// (src/osprey/packages/manager/host/src/host.osp), trimmed to two operations.
effect Files {
    read: fn(string) -> Result<string, Error>
    write: fn(string, string) -> Result<int, Error>
}

// Application logic: change the manifest, then write the lock file. If the
// lock write fails, put the old manifest back. It never touches a disk.
fn commitLock(previous, lock) = match perform Files.write("osprey.lock", lock) {
    Success _ => "installed"
    Error { message } => match perform Files.write("osprey.toml", previous) {
        Success _ => "rolled back: ${message}"
        Error _ => "manifest left changed: ${message}"
    }
}

fn install(manifest, lock) = match perform Files.read("osprey.toml") {
    Error { message } => "no manifest: ${message}"
    Success { value } => match perform Files.write("osprey.toml", manifest) {
        Error { message } => "manifest not written: ${message}"
        Success _ => commitLock(value, lock)
    }
}

// Production: the real disk.
let disk = handler Files {
    read path => readFile(path)
    write path text => writeFile(path, text)
}

// Test double: an in-memory disk holding one manifest. It records every
// write, and `full` makes the lock write fail.
fn memory(full) = {
    mut manifest = "version = 1"
    mut written = ""
    handler Files {
        read _ => Success { value: manifest }
        write path text => {
            let denied = full && path == "osprey.lock"
            manifest = if path == "osprey.toml" { text } else { manifest }
            written = if denied { written } else { "${written}${path} " }
            if denied { Error { message: "disk full" } } else { Success { value: byteLength(text) } }
        }
        return outcome => "${outcome} | ${written}| ${manifest}"
    }
}

fn upgrade() = install("version = 2", "locked")

test("a full disk rolls the manifest back", fn() =>
    expect(memory(true)(upgrade), "rolled back: disk full | osprey.toml osprey.toml | version = 1"))
test("a healthy disk writes manifest then lock", fn() =>
    expect(memory(false)(upgrade), "installed | osprey.toml osprey.lock | version = 2"))
```

```text
ok 1 - a full disk rolls the manifest back
ok 2 - a healthy disk writes manifest then lock
1..2
# tests=2 passed=2 failed=0 skipped=0
```

- `effect Files` lists the operations and their types. It's the interface to the outside world.
- `install` and `commitLock` call `perform Files.read(...)` and `perform Files.write(...)`. They don't take a file-system parameter, and they never name the disk.
- `disk` is the production handler. `disk(upgrade)` runs `upgrade` against real files.
- `memory(full)` is a test double. It builds a fresh in-memory disk, records every write, and can be told to fail the lock write. Its `return` clause adds what it saw to the result.

## Test doubles are just handlers

You don't need a mocking library, dependency injection or an interface threaded through every call. To control a side effect in a test, run the code under a different handler:

- **Stub:** return canned answers, for example a fixed manifest from `read`.
- **Fake:** keep working state in the handler, like the in-memory `manifest` above.
- **Spy:** record what the code asked for, like `written` above. The test can then check the order of the writes.
- **Failure injection:** return `Error` from the operation you want to break. That's how the first test checks the rollback.

The code under test is the same code that runs in production. If someone breaks the rollback (for example, by restoring the manifest to the wrong path), the test reports it:

```text
# expect failed: expected rolled back: disk full | osprey.toml osprey.toml | version = 1, got rolled back: disk full | osprey.toml x | version = 2
not ok 1 - a full disk rolls the manifest back
```

## A real test from the package manager

The package manager's CLI tests never touch a real disk, process or terminal. This excerpt is copied verbatim, at build time, from [`dispatch_tests.osp`](https://github.com/Nimblesite/osprey/blob/main/{{ effectsTestExample.file }}). `healthyFiles` and `brokenFiles` are two stubs for the same `Host::Files` effect shown above. `doctorBroken` runs the real `osprey-pkg doctor` command against a host where every file write and process fails, then checks that the report names each failure:

```osprey
{{ effectsTestExample.excerpt | safe }}
```

`osprey-pkg doctor` performs `Host::Files.write` and `Host::Process.run` just as it does in production. The nested calls install one test handler per effect (`Files`, `Process`, `Tree`, `Web`) around it. Run the suite with:

```sh
osprey test src/osprey/packages/manager/cli/tests --memory=gc
```

```text
ok 4 - dispatch: doctor on a broken host
...
# tests=98 passed=98 failed=0 skipped=0
```

For an ML-flavor spy, see [`accounts.test.ospml`](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/projects/modules/test/accounts.test.ospml). Its `guardCase` handler sets `touched` if storage is called, and the test asserts it stays `false` for an invalid payment.

## Python flavor

In Python flavor, `@effect` declares the effect, `perform(...)` asks for an operation, and `@handler(Files)` turns a class into a handler. Run work under a handler by calling it, `full_disk(upgrade)`, or with a `with` block:

```python
# osprey: flavor=python

# The package manager's file-system effect, in Python flavor.
@effect
class Files:
    def read(path: str) -> Result[str, Error]: ...
    def write(path: str, text: str) -> Result[int, Error]: ...


def commit_lock(previous, lock):
    match perform(Files.write("osprey.lock", lock)):
        case Success():
            return "installed"
        case Error(message=message):
            match perform(Files.write("osprey.toml", previous)):
                case Success():
                    return f"rolled back: {message}"
                case Error():
                    return f"manifest left changed: {message}"


def install(manifest, lock):
    match perform(Files.read("osprey.toml")):
        case Error(message=message):
            return f"no manifest: {message}"
        case Success(value=previous):
            match perform(Files.write("osprey.toml", manifest)):
                case Error(message=message):
                    return f"manifest not written: {message}"
                case Success():
                    return commit_lock(previous, lock)


# Production: the real disk.
@handler(Files)
class disk:
    def read(path):
        return readFile(path)

    def write(path, text):
        return writeFile(path, text)


# Test doubles: canned disks, no files touched.
@handler(Files)
class healthy_disk:
    def read(_path):
        return Success(value="version = 1")

    def write(_path, text):
        return Success(value=byteLength(text))


@handler(Files)
class full_disk:
    def read(_path):
        return Success(value="version = 1")

    def write(path, text):
        return Error(message="disk full") if path == "osprey.lock" else Success(value=byteLength(text))


@handler(Files)
class read_only_disk:
    def read(_path):
        return Success(value="version = 1")

    def write(_path, _text):
        return Error(message="read-only")


def upgrade():
    return install("version = 2", "locked")


def main():
    test("a healthy disk installs", lambda: expect(healthy_disk(upgrade), "installed"))
    test("a full disk rolls the manifest back", lambda: expect(full_disk(upgrade), "rolled back: disk full"))
    with read_only_disk:
        test("a read-only disk changes nothing", lambda: expect(upgrade(), "manifest not written: read-only"))
```

```text
ok 1 - a healthy disk installs
ok 2 - a full disk rolls the manifest back
ok 3 - a read-only disk changes nothing
1..3
# tests=3 passed=3 failed=0 skipped=0
```

Each test double is a small class with one method per operation. `install` is ordinary Python-shaped code that has no idea which disk it is using. See [Python flavor](/docs/python/flavor/#effects-and-handlers) for the full syntax.

{{ capture("/assets/images/guides/effects-python-doubles.png", "The Playground running the Python-flavor Files test doubles: healthy_disk, full_disk and read_only_disk handlers, and three passing TAP results.", "The Python test doubles above, run in the Playground: three handlers, three passing tests.", 1440, 780) }}

## ML flavor

```osprey-ml
// The file-system effect from the package manager's host boundary
// (src/osprey/packages/manager/host/src/host.osp), trimmed to two operations.
effect Files
    read : string => Result<string, Error>
    write : (string, string) => Result<int, Error>

// Application logic: change the manifest, then write the lock file. If the
// lock write fails, put the old manifest back. It never touches a disk.
commitLock previous lock = match perform Files.write "osprey.lock" lock
    Success _ => "installed"
    Error message => match perform Files.write "osprey.toml" previous
        Success _ => "rolled back: ${message}"
        Error _ => "manifest left changed: ${message}"

install manifest lock = match perform Files.read "osprey.toml"
    Error message => "no manifest: ${message}"
    Success value => match perform Files.write "osprey.toml" manifest
        Error message => "manifest not written: ${message}"
        Success _ => commitLock value lock

// Production: the real disk.
disk = handler Files
    read path => readFile path
    write path text => writeFile path text

// Test double: an in-memory disk holding one manifest. It records every
// write, and `full` makes the lock write fail.
memory full =
    mut manifest = "version = 1"
    mut written = ""
    handler Files
        read _ => Success(value = manifest)
        write path text =>
            manifest := match path == "osprey.toml"
                true => text
                false => manifest
            match full && path == "osprey.lock"
                true => Error(message = "disk full")
                false =>
                    written := "${written}${path} "
                    Success(value = byteLength text)
        return outcome => "${outcome} | ${written}| ${manifest}"

upgrade () = install "version = 2" "locked"

test "a full disk rolls the manifest back" (\() => expect (memory true upgrade) "rolled back: disk full | osprey.toml osprey.toml | version = 1")
test "a healthy disk writes manifest then lock" (\() => expect (memory false upgrade) "installed | osprey.toml osprey.lock | version = 2")
```

## A missing handler is a compile error

Remove `disk(...)` around a call that performs `Files`, and the compiler stops before anything runs:

```text
unhandled effect operations at program entry: Files.read; add a matching `handle`
```

This is checked through helpers, lambdas and fibers, so production code can't accidentally run without its real implementation, and a test can't accidentally touch the real one.

{{ capture("/assets/images/guides/effects-unhandled.png", "The Playground rejecting the Files example called with no handler: unhandled effect operations at program entry: Files.read, Files.write.", "The Files example called with no handler: compilation fails before anything runs.", 1440, 780) }}

## Other handler forms

`handle` installs a handler for the rest of a block:

```osprey
effect Files { read: fn(string) -> Result<string, Error> }
fn manifest() = perform Files.read("osprey.toml") ?: "missing"

let shown = {
    handle Files { read _ => Success { value: "version = 3" } }
    manifest()
}
print(shown)
```

A `control` operation also lets the handler stop the code that made the request, or act after it finishes; see [stopping or continuing the code that asked](#stopping-or-continuing-the-code-that-asked).

`handle static` resolves a value handler at compile time, so no handler lookup happens at run time. GPT-2 uses this to choose its [CPU or GPU tensor backend](/docs/python/flavor/#cpu-or-gpu-is-a-handler).

```osprey
effect DraftConfig { autosaveSeconds: fn() -> int }
fn relay(callback) -> int !e = callback()
fn work() !DraftConfig = perform DraftConfig.autosaveSeconds()

let answer = {
    handle static DraftConfig { autosaveSeconds => 30 }
    relay(work)
}
print(answer)
```

Integer arithmetic performs the built-in `Arith` effect on overflow and division by zero, so you choose the policy. For example, `handler Arith { overflow _ _ _ wrapped => wrapped }` wraps. See [arithmetic effects](/spec/0037-arithmeticeffects/).

## Stopping or continuing the code that asked

Most requests work like a function call: the handler returns an answer, and the code that asked carries on. `Files.read` is like that. Sometimes the handler needs more control. It may need to stop the work altogether, or to do something after the work has finished. Declaring the operation `control` gives its handler both choices:

- **`resume(value)`** carries on with the code that asked, handing it `value`. `resume` returns only when that code has finished, so the handler can clean up afterwards.
- **Not calling `resume`** stops the work at the request. The handler's own value becomes the result, as if the work had returned it.

### Example: locking the project during an install

Two installs running at once on the same project would both write `osprey.lock`, so a package manager locks the project before it starts. Here `install` asks for the lock and never releases it itself:

```osprey
effect ProjectLock { control acquire: fn(string) -> Unit }
effect Console { line: fn(string) -> Unit }

fn install() = {
    perform ProjectLock.acquire("osprey.toml")
    perform Console.line("writing osprey.lock")
    "installed"
}

// Hold the lock while the rest of the install runs, then release it.
let free = handler ProjectLock {
    acquire path => {
        perform Console.line("locked ${path}")
        let outcome = resume()
        perform Console.line("unlocked ${path}")
        outcome
    }
}
// Another install holds the lock: stop this one before it writes anything.
let busy = handler ProjectLock {
    acquire path => "stopped: another install holds ${path}"
}
let terminal = handler Console { line text => print(text) }

print(terminal(fn() => free(install)))
print(terminal(fn() => busy(install)))
```

```text
locked osprey.toml
writing osprey.lock
unlocked osprey.toml
installed
stopped: another install holds osprey.toml
```

- `free` takes the lock, calls `resume()` to run the rest of the install, and releases the lock once `resume` returns. `install` cannot forget to unlock, and neither can any function it calls.
- `busy` does not resume. The install stops at its first line, nothing is written, and the caller gets the reason as an ordinary value. There is no exception to catch.
- A test installs `busy` to check the "already running" path without starting two processes.

[`@osprey/numpy`](https://github.com/Nimblesite/osprey/blob/main/src/osprey/packages/numpy/src/errors.py) stops work the same way: its `ValueError.fail` is a control operation, so a shape mismatch deep inside a matrix product stops the whole calculation and the caller receives numpy's error message as an `Error`.

{{ capture("/assets/images/guides/effects-resume.png", "The Playground running the project-lock example: locked osprey.toml, writing osprey.lock, unlocked osprey.toml, installed, then stopped: another install holds osprey.toml.", "The lock example in the Playground: free resumes the install and unlocks after it; busy stops it.", 1440, 780) }}

### How many times a handler may resume

A control operation also declares how many times its handler may call `resume`. The compiler checks every handler against it.

| Declaration | The handler may resume | Status |
|---|---|---|
| `control acquire` (same as `control once acquire`) | At most once | Works on native targets |
| `control many pick` | Any number of times | Planned; rejected at compile time today |
| `control abort quit` | Never | Planned; rejected at compile time today |

A `once` handler may resume zero times, as `busy` does. It may also try something twice before resuming, for example retrying a payment, as long as each path through it resumes at most once. In Python flavor, mark the operation `@control`, then `@once`, `@many` or `@replayable`.

### Resuming more than once: backtracking (planned)

Choosing package versions is a search. Try the newest `serde`; if a package chosen later conflicts with it, go back and try an older `serde`. With `control many`, the handler can return to the same request and answer it again with a different version:

```osprey
effect Versions { control many pick: fn(string) -> string }

fn plan() = {
    let serde = perform Versions.pick("serde")
    let toml = perform Versions.pick("toml")
    if serde == "2" && toml == "2" { "conflict" } else { "serde ${serde}, toml ${toml}" }
}

// Try the newest version; if the rest of the plan conflicts, go back and try the older one.
let newestThatWorks = handler Versions {
    pick _ => match resume("2") {
        "conflict" => resume("1")
        found => found
    }
}

print(newestThatWorks(plan))
```

The [specification](/spec/0017-algebraiceffects/) defines the answer as `serde 2, toml 1`: the first attempt conflicts, so the handler resumes the `toml` request again with `"1"`. Each resumption starts from the same point with its own copy of the state. Today native `resume` cannot rerun the same point, so the compiler rejects the program instead of running it wrongly:

```text
11:4: `Versions.pick` is declared `many`, but a re-entrant continuation does not exist: native resume is one suspended stack, switched to and never switched back (docs/plans/0016-algebraic-effects-and-handlers.md). Declare `Versions.pick` `once`
```

The safety rules for backtracking already work. Going back reruns everything after the request, so a `many` handler rejects code that does something unsafe to repeat. Writing the lock file inside the search would write it once per attempt:

```osprey
effect Versions { control many pick: fn(string) -> string }
effect Files { write: fn(string, string) -> Result<int, Error> }

fn plan() = {
    let serde = perform Versions.pick("serde")
    match perform Files.write("osprey.lock", "serde = ${serde}") {
        Success _ => "locked serde ${serde}"
        Error { message } => message
    }
}

let locked = {
    handle Files { write path text => writeFile(path, text) }
    handle Versions { pick _ => match resume("2") { "conflict" => resume("1")  found => found } }
    plan()
}
print(locked)
```

```text
14:22: handler for `Versions.pick` may resume more than once, but the handled expression requires non-replayable effect `Files.write`; the whole handled expression's row is read, so move the non-replayable work outside the region
```

Write the lock after the search finishes, or mark an operation `replayable` when repeating it is harmless. A `many` handler also cannot change a `mut` variable, and a search cannot span a spawned fiber. [Plan 0016](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0016-algebraic-effects-and-handlers.md) tracks the work.

## Where effects are used

- [Talon Bank](/docs/web-apps/#algebraic-effects-what-runs-where) uses handlers for server storage and auditing, and for its browser boundary.
- [Issue Inbox](/docs/mobile-apps/#application-effects) turns application requests into commands for its Swift and Android hosts.
- [Runnable handler examples](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/handlers) cover factories, control flow, return clauses and static selection.

[Feature status](/status/#algebraic-effects) lists what remains unfinished. The [effects specification](/spec/0017-algebraiceffects/) defines the full contract.
