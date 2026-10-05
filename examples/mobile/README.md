# Osprey Issue Inbox

An iPhone and Android application using algebraic effects for SQLite and HTTP requests. Shared Osprey code owns state, issue decoding, local notes, bookmarks, and the UI description. A callable handler turns typed requests into commands for SwiftUI and Android hosts.

For a banking workflow shared across web, Android and iOS, see [Talon Bank's native apps](../projects/modules/mobile/README.md).

<p align="center">
  <img src="../../website/src/assets/images/mobile/issue-inbox-ios.png" alt="Issue Inbox showing live GitHub issues on the iPhone 17 Pro simulator" width="280" />
  <img src="../../website/src/assets/images/mobile/issue-inbox-android.png" alt="Issue Inbox restoring cached GitHub issues on the Pixel 7 Android emulator" width="280" />
</p>

Earlier captures from the iPhone simulator (left) and Android emulator (right), showing live issues and a restored SQLite cache.

```mermaid
flowchart LR
    I[iPhone native UI] -->|events| O[Shared Osprey application]
    A[Android native UI] -->|events| O
    O -->|UI tree and opaque state| I
    O -->|UI tree and opaque state| A
    O -->|SQL and HTTPS commands| H[Native platform services]
    H --> S[(SQLite cache)]
    H --> G[GitHub REST API]
    H -->|completion events| O
```

The native shells render the screen and execute platform services. Application rules and layout live in [`inbox/src/`](inbox/src/).

## Effects at the host boundary

[`Storage::Requests`](inbox/src/storage.ospml) declares two value operations: `sql` accepts a record containing a request ID, statement, and bound parameters; `http` accepts a request ID and URL. [`Update`](inbox/src/update.ospml) performs them while computing the next immutable state:

```ospml
result pending (perform Storage::Requests.http (Storage::fetch pending))
```

The production policy is a callable handler inside `Storage::commands`:

```ospml
export commands work =
    native = handler Requests
        sql request => sqlCommand request
        http request => httpCommand request
    native work
```

[`App`](inbox/src/app.ospml) selects that policy around each transition: `Storage::commands Update::initial`, or `Storage::commands (\() => Update::dispatch model event)`. Intermediate application functions need no service parameter. The callback lets the handler install its policy before the transition runs.

Tests can run the same transition under another handler. A callable handler in [the domain suite](inbox/test/main.ospml) records schema requests; a rest-of-block `handle Storage::Requests` replaces HTTP command generation and verifies request correlation. Existing tests also exercise the production policy's SQL parameters and JSON envelope.

These value operations return command descriptions synchronously. Swift and Kotlin execute them after the call returns, then deliver completion events. No continuation crosses the C boundary, and this example does not depend on the planned staged reactive runtime.

## iPhone and simulator

Use an Apple silicon Mac with Xcode, the iOS SDKs, an installed iPhone simulator, and the repository's Rust/LLVM prerequisites. From the repository root:

```sh
make _runtime_ios _runtime_ios_sim
cargo build --release -p osprey-cli
examples/mobile/ios/run.sh ios-sim
examples/mobile/ios/run.sh --smoke
```

Build an unsigned device application with `examples/mobile/ios/run.sh --build ios`. To install a signed build on your connected iPhone:

```sh
OSPREY_DEVELOPMENT_TEAM=YOUR_TEAM_ID examples/mobile/ios/run.sh ios-device
```

Set `OSPREY_DEVICE_ID` to select a physical device, or `OSPREY_SIMULATOR_UDID` to select a simulator. `OSPREY_BIN` selects another compiler binary. The mobile launcher reuses [`../ios/`](../ios/) build/deployment scripts with the shared project, app scheme, and bundle identifier supplied through environment settings.

## Android

The Android application lives in [`android/`](android/), with Kotlin native rendering and a JNI adapter for the generated C interface. It requires JDK 17, Android SDK 34, an installed Android NDK, and an emulator/device running API 26 or later. Its launcher uses a local Gradle installation when available and downloads Gradle 8.7 otherwise. From the repository root:

```sh
make android
examples/mobile/android/run.sh
make android-test
```

`examples/mobile/android/build.sh` builds directly; `examples/mobile/android/run.sh --smoke` runs deterministic fixture and cache-restart checks. Use `--live-smoke` to check actual GitHub responses separately. Set `ANDROID_HOME` or `ANDROID_SDK_ROOT` for the SDK, `ANDROID_NDK_HOME` or `ANDROID_NDK_ROOT` to select an NDK, and `OSPREY_ANDROID_SERIAL` for a particular connected device. The scripts can discover an installed NDK. Set `OSPREY_ANDROID_SKIP_BUILD=1` to install/launch an existing build. The APK includes ARM64 and x86-64 libraries; the launch script requires one connected device unless a serial is supplied.

## Shared application

| Source | Responsibility |
| --- | --- |
| `inbox/src/model.ospml` | Opaque application state and versioned snapshot |
| `inbox/src/update.ospml` | Events, command sequencing, and error handling |
| `inbox/src/annotations.ospml` | Local notes, priorities, and annotation limits |
| `inbox/src/github.ospml` | Repository validation, API URL, response decoding |
| `inbox/src/storage.ospml` | Typed request effect, callable command handler, SQLite schema and bound statements |
| `inbox/src/view.ospml` | Search, saved filter, counts, and issue presentation |
| `inbox/src/markdown.ospml` | Issue description Markdown to UI nodes and rich spans |
| `inbox/src/ui.ospml` | Native UI tree, labels, layout, and action events |
| `inbox/src/app.ospml`, `main.osp` | Handler selection, envelope assembly, and C-callable start/dispatch wrappers |

`osprey_mobile_start()` and `osprey_mobile_dispatch(model, event)` return JSON containing an opaque model string, structured view, UI tree, and platform commands. Hosts copy the returned string immediately and run emitted SQL/HTTP commands in order. Each completion returns to Osprey as another event. The hosts do not implement issue filtering, bookmarks, SQL selection, or GitHub response decoding.

SQLite stores a versioned inbox snapshot in application storage. A populated cache opens without fetching; Refresh explicitly requests current issues. Successful responses, bookmarks, notes, and priorities are saved. Search/filter choices and the open detail screen remain transient. Failed refreshes keep the previous issues and show the error. Notes and priorities stay in this inbox and never modify the GitHub issue.

Choose Details on a card to read its description and labels, set Normal/High priority, or edit a local note and press Done to save. Descriptions are Markdown: Osprey turns headings, lists, task lists, quotes, fenced code, rules, emphasis, inline code and HTTPS links into UI nodes that both hosts draw natively (`inbox/src/markdown.ospml`). Descriptions show at most 2,000 characters. Notes have the same limit and the cache supports annotations on at most 200 issues.

<p align="center">
  <img src="../../website/src/assets/images/mobile/issue-inbox-ios-detail.png" alt="iOS issue details with native light appearance, local triage controls, and rendered Markdown" width="280" />
  <img src="../../website/src/assets/images/mobile/issue-inbox-android-detail.png" alt="Android issue details with labels, local priority, notes, and the issue description" width="280" />
</p>

The detail screen on the iOS simulator (left) and Pixel 7 emulator (right). Layout, labels, selected issue, Markdown rendering, note validation, priority changes, and persistence commands come from Osprey.

The API requests one page of open issues from `swiftlang/swift` initially; enter another `owner/repository` to change it. Osprey excludes pull requests because [GitHub's issue endpoint includes them](https://docs.github.com/en/rest/issues/issues#list-repository-issues). No GitHub credential is required for this public-data example. Authentication, pagination, background refresh, and posting changes to GitHub are not implemented.

## Assertions and diagnostics

From the repository root, `make mobile-domain-test` runs the shared Osprey assertions, `make mobile-ios-test` builds and checks the iOS application, and `make mobile-test` runs shared, iOS, and Android verification together. The combined target requires both platform toolchains and running test devices.

Native smoke tests use a separate SQLite database and exercise the real Osprey archive. iOS uses deterministic HTTP completions to check bound SQL, cache persistence, search, saved filters, bookmarks, detail/annotation events, and visible HTTP failures. It writes `OSPREY_INBOX_SMOKE_OK` to `Documents/inbox-smoke-result.txt`; the launcher requires a fresh result. Android's deterministic smoke exercises the shared app, terminates the process, and verifies its saved cache after restart. Its additional `--live-smoke` mode requires network access and available GitHub rate limits.

For a simulator that already has the app installed, enable local envelope diagnostics with:

```sh
xcrun simctl launch --terminate-running-process booted org.ospreylang.IssueInbox --inbox-diagnostics
xcrun simctl get_app_container booted org.ospreylang.IssueInbox data
```

Use the returned container path to read `Documents/inbox-state.json`. With multiple booted simulators, replace `booted` with the chosen UDID. Adding `--inbox-open-first` opens the first loaded issue automatically, which is how the Markdown detail screenshot is captured with `xcrun simctl io booted screenshot`. Diagnostic output is disabled in ordinary launches.

The native C boundary currently uses the default memory runtime and retains general allocations for the process lifetime. Hosts copy strings but cannot release Osprey allocations through a stable public API yet. Mobile targets reject unsupported features such as resumable effects and built-in native HTTP; platform networking runs through the host command boundary. See the [iOS target](../../docs/specs/0038-iOSTarget.md), [Android target](../../docs/specs/0039-AndroidTarget.md), [application specification](../../docs/specs/0040-ReactiveMobileApplications.md), and [delivery plan](../../docs/plans/0030-reactive-mobile-apps.md).

## Validation

Run the shared domain suite without a device:

```sh
target/release/osprey examples/mobile/inbox/test --run
```

The suite covers handler substitution, command generation, state transitions, offline cache restoration, errors, response correlation, notes, priorities, JSON escaping, Unicode, and Markdown rendering. Platform builds and device smoke checks require the toolchains above. Earlier simulator/device screenshots do not verify later source changes; rerun the native checks after changing the application or compiler.
