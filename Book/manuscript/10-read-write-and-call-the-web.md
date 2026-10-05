# Chapter 10 — Read, write, and call the web

The Flight Log can describe a learning goal. Now let it remember that goal after the program ends. The result we want is small and visible: save `Build a small Osprey CLI`, read it back, and produce `Flight Log: Build a small Osprey CLI` again.

A file brings new possibilities. It may be missing. Writing may fail. Reading may succeed while the contents are unsuitable. An effect gives us a place to choose how storage works; a `Result` gives us a way to describe what happened. We need both.

This chapter builds one deliberately small boundary: a file containing a single title. It then follows the same idea to an HTTP request and to the native hosts used by the banking and mobile examples. You will be able to explain where outside work happens, where incoming data is checked, and what a deterministic test actually proves.

## Give storage a small contract

Here is the storage interface and the function that asks it to load a title. These are excerpts from the complete `examples/chapter-10/flight-log.osp`:

```osprey
effect Store {
    read: fn() -> Result<string, Error>
    write: fn(string) -> Result<int, Error>
}

fn load() = match perform Store.read() {
    Success { value } => decodeTitle(value)
    Error { message } => Error { message: message }
}
```

`Store.read` asks for text. `Store.write` supplies text and, on success, receives a byte count. Neither operation takes a path. This application has one title to store, so the handler can own its location. The core does not need permission to select arbitrary files.

The declared result has two routes. `Success` carries the requested value. `Error` carries information about a failure. The built-in file operations use the error type spelled `Error`, so this contract matches them directly. In a pattern such as `Error { message }`, `message` is the explanation that the program can retain or display.

The effect and the result answer different questions. The effect asks, “Which implementation should perform this operation here?” The result asks, “Did that implementation succeed?” Installing a handler does not make the disk reliable. Returning an `Error` does not leave the effect unhandled: the handler answered the operation with a valid failure value.

Every operation in this chapter is a **value operation**. Its handler arm returns a value to the `perform` call. There is no `control` declaration and no `resume`. Expected file and request failures need ordinary branching over results.

`load` contains the first important sequencing decision. It decodes only a successful read. A read error stays an error with its original message. There is no blank-string fallback to turn “permission denied” into “the learner saved an empty title.”

The compiler checks the declared operations and required handlers, including requests reached through helper calls. A storage interface does not itself configure an operating-system sandbox. The native handler still runs with the access available to the process; runtime sandbox options are a separate control.

## Turn outside text into a checked value

A successful read tells us that text arrived. It does not tell us that the text is a usable Flight Log title. Put that decision in a function which performs no outside work:

```osprey
type Entry = { title: string }

fn decodeTitle(raw) = {
    let title = trim(raw)
    if title == "" {
        Error { message: "title is empty" }
    } else if contains(title, "\n") || contains(title, "\r") {
        Error { message: "title must be one line" }
    } else {
        Success { value: Entry { title: title } }
    }
}

fn summary(entry) = "Flight Log: ${entry.title}"
```

**Decoding** means turning an outside representation into a value the program can use. Here the representation is plain text, so decoding is short. Remove surrounding whitespace, reject an empty title, reject an internal line break, and construct an `Entry`. We do not need JSON, a database schema, or a serialization framework to learn the boundary.

The order is deliberate. A title file ending with a newline is convenient to edit and remains acceptable after trimming. Two lines with actual text remain two lines and are rejected. This is the file format's rule; it is not a universal rule for everything called a title.

`decodeTitle` and `summary` are **pure**: their results depend on their inputs, and they request no file, clock, or network work. Given the same text, the decoder makes the same decision. Given the same entry, the summary produces the same string.

![Storage returns a read error or raw text. Decoding then returns a validation error or an Entry; only the Entry reaches the pure summary.](assets/diagrams/10-boundary-path.png)

*Figure 10.1 — Receiving data and accepting data are separate decisions. Each failure retains a route back to the caller.*

The `Entry` record says that a title is a string. Its declaration alone does not prohibit an empty string. Our loading path establishes the additional rule by constructing its entry through `decodeTitle`. If another part of the program constructs an entry directly, that part must respect the same rule. Do not credit the type checker with validation that the program has not expressed.

The supplied `preview` function matches `load`: it sends a successful entry to `summary`, or produces `Cannot load: ${message}`. That is the display boundary. The lower functions preserve a failure as data until there is a real decision about presenting it.

## Change the handler to explore failure

Before depending on a disk, run the workflow against values under your control. The example includes this factory:

```osprey
fn fixture(raw) = handler Store {
    read => Success { value: raw }
    write text => if text == "Build a small Osprey CLI" {
        Success { value: byteLength(text) }
    } else {
        Error { message: "unexpected title" }
    }
}
```

A **fixture** is known input for an example or test. Calling `fixture("Build a small Osprey CLI")` creates a handler whose read operation supplies that text. Calling the resulting handler with `preview` runs the loading workflow under that implementation.

As in Chapter 9, pass the function: `ready(preview)`. Calling `preview()` first would begin the work before `ready` had installed its handler. An extra `fn() => preview()` adds no work here. A callback is useful because it controls when the work begins; a forwarding wrapper is unnecessary when the function already has the required shape.

The fixture's write arm checks the actual text supplied by the application. An accidental change to the save payload produces an error instead of a false success. However, this fixture does not remember writes: its read always returns the captured `raw`. It proves behavior against a script. It does not prove persistence.

The complete example supplies additional handlers for an empty title, a read denied by policy, and a full disk. The exact fixture output lines are:

| Case | Output |
|---|---|
| Valid read | `fixture: Flight Log: Build a small Osprey CLI` |
| Whitespace-only contents | `invalid: Cannot load: title is empty` |
| Failed read | `read error: Cannot load: permission denied` |
| Failed write | `write error: Cannot save: disk full` |
| Accepted fixture write | `fixture save: Saved 24 bytes; Flight Log: Build a small Osprey CLI` |

These failures require no broken machine and no changes to file permissions. They make the application's reactions reproducible. Chapter 8's output checks or assertions can then distinguish the intended result from a regression.

Try changing the invalid fixture to `"First\nSecond"`. Predict the new explanation before running it. The answer is `title must be one line`: trimming cannot remove the internal newline. Changing it to `"  Build a small Osprey CLI  "` instead produces the ordinary summary, because surrounding spaces are removed.

The fixture for failed writes also supplies a deliberately conspicuous read failure. The save workflow should never reach it after a failed write. This makes an incorrect “continue anyway” change visible in the output.

## Save and read back with a native adapter

An **adapter** translates one interface into another. The native storage handler translates our two operations into the existing file builtins:

```osprey
fn saveAndLoad() =
    match perform Store.write("Build a small Osprey CLI") {
        Success { value } => "Saved ${value} bytes; ${preview()}"
        Error { message } => "Cannot save: ${message}"
    }

fn nativeExample() = {
    let native = handler Store {
        read => readFile("flight-log.txt")
        write text => writeFile("flight-log.txt", text)
    }
    native(saveAndLoad)
}
```

The workflow calls `preview` only after a successful write. That leads through `load`, the selected read arm, the decoder, and the pure summary. The implementation changes at the handler; those helpers keep their existing definitions.

`writeFile` writes or replaces its target. Do not run this example in a directory containing a file you care about with that name. From the `Book` directory, these commands use a fresh temporary directory and remove it afterwards:

```sh
book_compiler="$(cd .. && pwd)/target/release/osprey"
book_source="$PWD/examples/chapter-10/flight-log.osp"
(
    book_tmp=$(mktemp -d)
    trap 'rm -r "$book_tmp"' EXIT
    cd "$book_tmp"
    "$book_compiler" "$book_source" --run
)
```

The program prints the five fixture lines above followed by `native save: Saved 24 bytes; Flight Log: Build a small Osprey CLI`. Its file contains exactly the title, with no newline. The byte count is 24 for this ASCII fixture. In general, bytes and human-visible characters are different measures, so the adapter uses the file builtin's byte count.

This is the Flight Log checkpoint. The scripted handler demonstrates the application's decisions; the native handler demonstrates an actual write, read, and decode. Matching summaries establish that both supplied the same accepted title. Neither check proves that every future filesystem will succeed.

A successful write is also not a transaction or a guarantee against a later machine failure. This small format replaces one file. Concurrent writers, crash recovery, and migrations need additional design. Add those requirements when the application needs them, rather than quietly implying they follow from `Success`.

File builtins are available in the native runtime. WebAssembly file access depends on what its WASI host exposes and permits. A browser application does not thereby gain unrestricted access to the reader's files. Keep the selected handler and host capabilities together when choosing a deployment target.

## Apply the same boundary to HTTP

A web request adds another distinction: transport can succeed while the server reports a failure status. Receiving HTTP 404 means a server answered “not found.” It is different from being unable to reach the server.

The offline `examples/chapter-10/web-title.osp` declares the request the application needs and runs it with three handlers:

```osprey
effect Web {
    title: fn() -> Result<string, Error>
}

fn download() = match perform Web.title() {
    Success { value } => "Received title: ${value}"
    Error { message } => "Cannot download: ${message}"
}
```

The successful handler supplies our fixed title. The others return `HTTP 404` and `connection unavailable` as their error messages. The output is respectively `Received title: Build a small Osprey CLI`, `Cannot download: HTTP 404`, and `Cannot download: connection unavailable`. The program makes no network request.

This example deliberately stops at receiving text. To use the text as a Flight Log entry, the success route must still call `decodeTitle`. A remote server can return an empty body just as a file can contain an empty title. `Success<string>` does not mean “valid entry.”

For an actual native request, `examples/chapter-10/native/http-title.osp` provides a complete adapter. Its companion README starts a local fixture server; no public service or account is needed. The important runtime operations are:

| Operation | Adapter responsibility |
|---|---|
| `httpCreateClient` | Check that a positive client handle was created. |
| `httpGetResponse` | Match its result to obtain a response handle or preserve a request error. |
| `httpResponseStatus` | Decide which status is acceptable; this example requires 200. |
| `httpResponseBody` | Obtain the body through another result. |
| `httpResponseFree` | Release every successfully obtained response exactly once. |
| `httpCloseClient` | Close the client after either request outcome. |

A **handle** is an identifier owned by the runtime. Obtaining one creates a cleanup obligation. The adapter attempts response cleanup after reading the status and body, including error routes, then closes the client. It also checks cleanup failures. These details belong in the adapter so every domain helper does not have to reproduce them.

Use `httpGetResponse` when you need the body. The shorter `httpGet` returns an integer status and discards the body. These native APIs do not all return the same type: creation, status, and close use integer conventions, while response acquisition and body access use `Result`. The adapter turns those different conventions into the one result promised by `Web.title`.

Native HTTP builtins are unavailable on the WebAssembly and mobile C ABI targets. The example's value-effect design remains useful there, but the host must supply networking. A direct C call is likewise a separate safety boundary, with argument, lifetime, and cleanup obligations; adding an effect does not make an unsafe foreign call safe.

## Connect this design to a mobile application

A native command-line request can return before the next expression runs. A phone interface must keep responding while networking continues. The banking and mobile examples therefore use an explicit command-and-event boundary.

An Osprey update computes a new model and describes an HTTP or storage command. The Android or iOS host executes that command using platform services. When it finishes, the host sends a completion event back to Osprey. A later update checks the result and computes the next model and view.

![An Osprey update produces a model and command; the native host performs the work and sends a completion event to a later Osprey update.](assets/diagrams/10-mobile-command.png)

*Figure 10.2 — The completion arrives as a new event. No suspended Osprey callback is kept alive across the network request.*

This is the practical connection to our title loader: keep validation and model decisions in Osprey, and choose outside implementations at a narrow boundary. It does not mean this chapter's synchronous `Store.read` can simply wait inside every mobile host. The mobile operation may return a command description; completion belongs to a later event.

Start with the bank's `examples/projects/modules/mobile/README.md` and the smaller `examples/mobile/README.md` when exploring that architecture. They document the actual host protocol. Do not infer support for mobile control continuations or a future reactive runtime from the presence of value-effect handlers.

When adding diagnostics, record the operation and outcome you need to understand the failure. Avoid printing entire HTTP headers, credentials, or private bodies as a debugging shortcut. Our local title is deliberately harmless; a banking payload needs a different presentation policy. Preserve useful failure information inside the program and decide deliberately what belongs on screen or in a log.

## Let the compiler expose a missing decision

The stored rejection example tries to treat the result of reading a file as an ordinary title:

```osprey
fn heading(title) = "Flight Log: " + title

fn main() = heading(readFile("flight-log.txt")) |> print
```

From `Book`, check it with `../target/release/osprey examples/chapter-10/failscompilation/undecoded-result.osp --check`. The diagnostic ends with the exact message `type mismatch: cannot unify string with Result<string, Error>`.

The requirement comes from string concatenation in `heading`. The supplied argument is a result containing either text or an error. Changing an annotation cannot make those possibilities disappear, and the compiler rejects this without reading the file.

The repair is to match the read result. Preserve the error route; send successful text to the decoder; match again before summarizing an entry. The `load` and `preview` functions already demonstrate that answer. A fallback such as `?: ""` would choose a different policy by hiding the read failure. It would make permissions trouble look like invalid contents.

### Agent handoff

```text
Extend the Chapter 10 Flight Log in Default flavor.
Keep decoding and summary pure. Request storage through Store.
Add a deterministic handler returning a two-line title and
verify that the decoder rejects it with the original message.
Keep the failed-read and failed-write cases in the output.
Run --check, then --run inside a fresh temporary directory.
Compare the entire output with the reviewed expected output.
Verify the saved file contains exactly the single title.
Report which checks used fixtures and which used the filesystem.
```

Review the evidence as well as the edited code. An agent should name the failure cases it exercised and show the actual output. A successful fixture run alone cannot establish that an adapter writes a real file. The book's example harness checks the stored programs and their exact outputs; the optional local HTTP adapter has separate instructions because it opens a socket.

## Landing check

- An effect selects an implementation; a `Result` preserves the operation's possible outcomes.
- Successful input still needs decoding before domain code treats it as an accepted value.
- Pure decoding and summaries are easy to exercise without a disk or network.
- Scripted handlers make success and failure deterministic, but they prove only their declared behavior.
- Native adapters own paths, status conventions, resource lifetimes, and cleanup.
- Mobile hosts perform commands and return completion events; their platform boundary must be designed explicitly.

The Flight Log can now cross a real boundary without concealing where work or failure happens. Chapter 11 introduces work that can proceed concurrently, while keeping ownership and communication explicit.

### Authoritative sources

- Osprey [Algebraic Effects](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0017-AlgebraicEffects.md): callable handlers, value operations, operation typing, and required handlers.
- Osprey [Built-In Functions](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0012-Built-InFunctions.md): `[BUILTIN-FILE]`, `[BUILTIN-FILE-ERRMSG]`, and string operations; [Error Handling](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0013-ErrorHandling.md): results and their preservation.
- Osprey [HTTP](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0014-HTTP.md): integer status conventions and `[HTTP-RESPONSE-HANDLE]`; [WebAssembly Target](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0022-WebAssemblyTarget.md): host-dependent file access and excluded networking APIs.
- Executable evidence: this chapter's sources and goldens; the runtime corpus at `tests/regressions/basics/files/file_io_json_workflow.test.osp` and `tests/regressions/http/http_response_handle.test.osp`; the native banking host protocol in `examples/projects/modules/mobile/README.md`.
