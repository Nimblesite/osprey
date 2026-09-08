# Reactive Mobile Applications

**Status:** implemented and verified on an ARM64 iOS simulator, a physical iPhone 16, and an ARM64 Android emulator. Android x86-64 is compile/link verified only. Compiler, application, and native integration checks pass, and the unchanged duplication gate passes with the Deslop 0.27.0 version CI pins.

The Issue Inbox sample shares its application state, repository validation, GitHub decoding, SQL statements, filtering, bookmarking, issue details, local notes/priorities, messages, and UI tree in Osprey. Swift and Kotlin provide native rendering and platform services. Both applications compile the same [`examples/mobile/inbox/`](../../examples/mobile/inbox/) project through the C ABI.

This is an application architecture using ordinary Osprey functions and event messages. It does not add a reactive compiler feature, a new UI language, or resumable effects. The platform target restrictions still apply.

## Native application boundary [MOBILE-NATIVE-HOST]

The host initializes the library through `osprey_main()`, then calls these generated exports on its UI thread:

```c
const char *osprey_mobile_start(void);
const char *osprey_mobile_dispatch(const char *model, const char *event);
```

Calls are synchronous. The host serializes calls, keeps input strings alive until return, and immediately copies the returned UTF-8 string. The host owns the resulting copy. iOS uses the generated C header through a Swift bridging header; Android uses a JNI adapter over the same C declarations. Memory and initialization follow the [iOS target](0038-iOSTarget.md) and [Android target](0039-AndroidTarget.md) specifications.

Every result is a JSON envelope containing `model`, `view`, `ui`, and `commands`. `model` is an opaque JSON **string** that the host returns unchanged with the next event. `view` is structured diagnostic data. `ui` is the native rendering description. `commands` is an ordered array of requested platform operations. Invalid JSON, missing fields, unsupported UI nodes, and invalid commands must be exposed as host errors rather than displayed as successful application updates.

## Reactive event processing [MOBILE-REACTIVE-HOST]

`mobile::start` produces initial state and the first commands. `mobile::dispatch` decodes the previous state and one event, applies `Update`, derives `View`, renders `Ui`, and returns the next envelope. The host stores the opaque model and publishes the new UI immediately, then executes emitted commands in order. Each completion is another event passed through the same dispatcher. Network work runs asynchronously in the host; there is no suspended Osprey call waiting for it.

UI events include refresh, repository submission, search changes, filter changes, bookmark toggles, issue opening/back navigation, note submission, and priority changes. The renderer forwards the event object supplied by Osprey; text inputs add the current `value`. It does not decide which issues match a query, whether a repository is valid, or what a bookmark or annotation means.

The Osprey model carries pending HTTP and SQL write identifiers. Only a completion matching the current request is applied. Older responses must not replace a newer repository selection or clear the status of a later save. UI events may arrive while HTTP is pending; each still invokes Osprey synchronously on the same thread.

Startup creates the schema and reads the stored snapshot. A valid snapshot opens offline, with refresh left to the user. An empty cache requests the default public repository, `swiftlang/swift`. Successful refreshes, bookmark changes, notes, and priorities persist a versioned snapshot. Search, the selected All/Saved filter, and the opened issue are transient. HTTP failures retain cached issue data. Database and malformed-cache errors remain visible. Notes and normal/high priorities belong to this local inbox and are never posted to GitHub.

## UI rendering [MOBILE-REACTIVE-UI]

The shared `Ui` module specifies labels, layout, input values, issue cards, empty states, status messages, actions, and links. Both native renderers understand `column`, `row`, `scroll`, `text`, `rich`, `button`, `input`, `link`, and `divider` nodes. Optional fields include `id`, `text`, `style`, `value`, `placeholder`, `url`, `submit`, `event`, `children`, and `spans`.

Native code maps these generic nodes and style names to SwiftUI or Android widgets. Platform typography, colors, keyboard handling, accessibility identifiers, and opening HTTPS links remain native responsibilities. Application-specific layout and behavior stay in Osprey. The platforms need not produce identical pixels, but must display the same state and send equivalent events for the same interactions.

The structured `view` contains repository, search, filter, status, error, loading state, visible issues, issue count, bookmark count, selected issue ID, and a nullable detail object. Each issue provides its stable string ID, issue number, title, URL, author, comment count, bookmark state, description, labels, note, and priority. Descriptions and notes are bounded to 2,000 Unicode scalar values, and search to 256; truncation must preserve complete UTF-8 sequences. Annotations are bounded to 200 issue IDs. This view supports diagnostics and assertions; the native renderer consumes `ui` for the actual screen.

The detail screen is also assembled in `Ui`: back navigation, issue metadata, labels and description, normal/high priority buttons, a note input submitted with Done, a bookmark action, and a GitHub link. Native code receives the same generic node types as the list screen, plus the `rich` node the description uses. Multiline note editing is not implemented.

## Issue description Markdown [MOBILE-MARKDOWN]

GitHub issue bodies are Markdown, and the shared `Markdown` module (`inbox/src/markdown.ospml`) turns a description into ordinary UI nodes before it reaches a host. Osprey decides the structure; the hosts only draw it. The `view` keeps the raw Markdown source so assertions and diagnostics can compare it with what the API returned.

Block syntax, one construct per line, recognised after leading whitespace is removed:

| Markdown | Node |
| --- | --- |
| `#` to `######` followed by a space | `rich` with style `heading` (levels 1 and 2) or `subheading` (3 to 6) |
| ```` ``` ```` fence until the closing fence | `text` with style `code`; lines and blank lines inside are kept verbatim |
| `- `, `* `, `+ ` | `row` with style `item`: a `text` bullet `•` and a `rich` body |
| `1. ` with any number | the same `row`, bullet `1.` |
| `- [ ] ` and `- [x] ` | the same `row`, bullet `☐` or `☑` |
| `> ` | `column` with style `quote` around a `rich` body |
| three or more `-`, `*` or `_` alone | `divider` with style `rule` |
| a blank line | ends the current paragraph |
| anything else | joins the current paragraph; a single newline stays a line break, as GitHub renders it |

An unclosed fence still renders as code, `#tag` without a space is text, and seven or more `#` are text.

A `rich` node carries `spans`, an ordered array of `{"text", "style", "url"}` objects. Inline syntax inside paragraphs, headings, list items and quotes: `**bold**`, `*italic*`, `` `code` `` and `[text](https://…)`. A span style is `plain`, `bold`, `italic`, `code` or `link`; only a link span has a non-empty `url`, and only `https://` URLs become links. A marker without a closing marker, an empty `****`, and a non-HTTPS link are literal text, so every description renders. An underscore pair is emphasis only at a word boundary, so GitHub's `_No Description_` placeholder is italic while `snake_case` identifiers stay literal. Nested emphasis is not interpreted; the inner text is shown as written.

Native renderers concatenate the spans into one text view: SwiftUI through `AttributedString` presentation intents and `link` attributes, Android through `StyleSpan`, `TypefaceSpan` and `URLSpan`. The host smoke fixtures include a bold marker in an issue body and assert that the rendered tree contains a bold span while the `view` still holds the raw Markdown. Tables, images, HTML, footnotes, task-list toggling and syntax highlighting are not implemented; they render as text.

## Platform commands [MOBILE-HOST-SERVICES]

| Command | Fields | Completion event |
| --- | --- | --- |
| SQLite | `kind: "sql"`, `id`, `sql`, `params` | `type: "sql"`, matching `id`, `ok`, `rows`, `error` |
| HTTPS GET | `kind: "http"`, `id`, `url` | `type: "http"`, matching `id`, `status`, `body`, `error` |

Osprey supplies the actual SQL and parameters. The host prepares one statement, verifies and binds its scalar parameters, executes it, and returns rows as JSON objects keyed by column name. Parameters support null, booleans, 64-bit integers, finite floating-point numbers, and UTF-8 strings. BLOB values and multiple statements are outside this transport contract. Values must never be interpolated into SQL text. The SQLite binding contract requires keeping bound bytes valid or asking SQLite to copy them; the iOS implementation uses `SQLITE_TRANSIENT`. See [SQLite parameter binding](https://sqlite.org/c3ref/bind_blob.html).

The initial schema is `app_state(key TEXT PRIMARY KEY, value TEXT NOT NULL)`. The `inbox` row holds Osprey's versioned snapshot. This is a durable application cache, not an in-memory substitute for SQL. Invalid snapshots are reported and replaced only by a subsequent successful refresh/save.

The HTTP host performs bounded HTTPS GET requests with a User-Agent and JSON Accept header, returning the HTTP status and raw UTF-8 body. It does not decode issues or decide whether an HTTP status is successful. Transport failures populate `error`. Requests have finite timeouts and a 2 MiB response limit. Authentication, pagination, retries, and background refresh are not implemented by this sample.

`Github` requests one page of open repository issues and validates the response in Osprey. It excludes entries with `pull_request`, because [GitHub's issue endpoint also returns pull requests](https://docs.github.com/en/rest/issues/issues#list-repository-issues). IDs and issue numbers must be positive; required text fields and comment counts are checked. Issue links are built from the validated repository and issue number.

## Host assertions [MOBILE-HOST-VERIFICATION]

Native smoke tests run the real application and C ABI against a separate SQLite database. They must not erase or replace the normal application cache. Both platforms provide deterministic HTTP completions containing issues and a pull request. Android also provides a separate live-network smoke mode and verifies a process restart against its saved cache.

The iOS smoke checks scalar SQL binding, Unicode and NUL text, parameter injection resistance, statement-count rejection, database reopen durability, initial Osprey rendering, pull-request exclusion, search, bookmarks, the saved filter, issue open/back events, note/priority persistence, cache reload without HTTP, and visible HTTP failure while retaining cached state. It writes `OSPREY_INBOX_SMOKE_OK` to `Documents/inbox-smoke-result.txt` only after all assertions pass. The launcher removes the previous marker and requires fresh output.

The Android smoke exercises shared state transitions through JNI, Android SQLite, and native rendering, including Unicode search, bookmarks, saved filtering, and cache restoration after terminating the process. Its launch script owns isolated test storage and validates fresh JSON results for the initial and restored phases. `--smoke` uses fixtures, while `--live-smoke` fetches actual public GitHub issues. A network outage or rate limit can fail the live mode without failing the offline fixture test. A build or successful process launch alone is not a passing application smoke test.

## Verification and diagnostics [MOBILE-VERIFICATION]

[`examples/mobile/README.md`](../../examples/mobile/README.md) contains the reproducible build and launch commands. Final acceptance requires shared Osprey checks, both native application builds, deterministic host smoke on both platforms, a live public GitHub response displayed by the application, and a subsequent cache-backed launch.

The shared application passed 29 Osprey domain assertions, including the Markdown rendering suites. The expanded Markdown smoke passed on the iOS simulator. The original application smoke passed inside the signed application on a physical iPhone 16; its diagnostics recorded eight live GitHub issues, saved bookmarks, and no application error. The subsequent Markdown build installed on that phone, but its launch check is waiting for the device to be unlocked. Android passed deterministic and live workflows, including detail navigation, notes/priorities, bookmarks, Unicode search, failed-refresh cache retention, and restoration after terminating and restarting the process. Its live workflow displayed eight issues. Native renderer regression checks and Android lint also passed.

Both mobile runtimes pass the shared C ABI fixture and the whole `tests/` corpus through the mobile C ABI on ARM64 — 130 byte-exact goldens and 18 GPU-lowering comparisons each, with every rejection pinned by name and reason in [`tests/MOBILE_UNPORTABLE.txt`](../../tests/MOBILE_UNPORTABLE.txt). The original iOS counter's `make ios-test` still passes. Compiler verification passed the Rust workspace suite, strict workspace Clippy, and formatting, and the duplication gate passes at 4.7% against its unchanged 5% ceiling under the Deslop 0.27.0 version CI pins. Android x86-64 archives and application libraries compiled and linked but were not executed in this validation.

On iOS, `--inbox-diagnostics` writes the latest complete envelope to `Documents/inbox-state.json`, and `--inbox-open-first` sends one ordinary `open` event for the first loaded issue once startup commands finish, so the Markdown detail screen can be captured without simulated taps. Both are explicit local debugging modes, disabled during ordinary launch. The envelope allows checking that the displayed UI, issue data, pending commands, and persisted application state came from Osprey. Screenshots provide visual evidence alongside those protocol assertions.

Source responsibilities are `inbox/src/{model,update,annotations,github,storage,view,ui,app}.ospml`, the scalar entry wrappers in `inbox/src/main.osp`, the Swift files in `ios/IssueInbox/`, and the Kotlin/JNI files in `android/app/src/main/`. Dedicated reactive dependency inference, platform GPU rendering, private GitHub authentication, full repository pagination, cache migration across future schema versions, and an Osprey string-release API are outside this delivery. The default runtime's general allocations remain alive for the process lifetime, so this is not yet a bounded-memory deployment contract.
