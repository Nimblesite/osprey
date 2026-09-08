# Plan 0030 — Shared Osprey reactive mobile application

**Subsystem:** `examples/mobile/inbox`, iOS Swift host, Android Kotlin/JNI host, and native C ABI targets.

**Status:** implementation and targeted validation complete. The shared application runs on a physical iPhone, the iOS simulator, and an ARM64 Android emulator. Android x86-64 compiled and linked but was not executed. Full `make ci` remains open: the unchanged duplication gate reported 9.4% against its 5% ceiling.

**Contract:** [Reactive Mobile Applications](../specs/0040-ReactiveMobileApplications.md), [iOS Target](../specs/0038-iOSTarget.md), and [Android Target](../specs/0039-AndroidTarget.md).

## Outcome

Deliver the same usable GitHub Issue Inbox on iPhone and Android. Osprey owns the state transitions, API decoding, SQL commands, filtering, bookmarks, issue details, notes/priorities, labels, and UI layout. Generic native hosts render that UI, execute platform commands, and return completion events. The app uses an event/update pattern supported by today's compiler; it does not depend on a dedicated reactive compiler feature or resumable effects.

## Work and evidence

| Work | Implementation | Acceptance evidence | Status |
| --- | --- | --- | --- |
| Shared state and update rules | `inbox/src/model.ospml`, `update.ospml`, `annotations.ospml` | Initial load, search, saved filter, detail navigation, annotations, request ordering, visible errors | Passed: 29 shared domain assertions and native workflows |
| GitHub input and persistence | `github.ospml`, `storage.ospml` | Pull-request exclusion, invalid-response rejection, bound SQL, cache reopen | Passed: fixture responses, real GitHub requests, SQLite recovery |
| Shared screen | `view.ospml`, `ui.ospml` | Both platforms display the same state and emit equivalent events | Passed: native rendering, interaction checks, and visual inspection |
| Scalar application boundary | `main.osp`, `app.ospml`, and generated C headers | Start/dispatch envelope crosses both native bridges | Passed: iOS device/simulator and Android ARM64/x86-64 builds; execution on ARM64 |
| Swift host | `ios/IssueInbox/` | Real SQLite, bounded HTTPS, main-thread dispatch, diagnostics | Passed: full simulator and physical-iPhone smoke, live GitHub diagnostics |
| Kotlin/JNI host | `android/app/src/main/` | Real SQLite, bounded HTTPS, UI-thread dispatch | Passed: deterministic/live smoke, process restart, renderer regression, lint |
| Delivery | Platform launch scripts and README | Repeatable simulator/emulator and signed-device launch | Passed: signed iPhone 16 installation/launch and emulator/simulator runs |

## Completion sequence

1. Finish compilation of the shared project and reject unsupported target features before LLVM emission.
2. Build the iOS and Android libraries and native applications from the same project.
3. Run deterministic smoke assertions on both platforms using isolated databases and canned HTTP completions, including Android's process-restart phase. A stale marker cannot pass.
4. Fetch live public GitHub issues and inspect the Osprey envelope and native screen. Confirm pull requests are excluded and HTTP errors are visible.
5. Relaunch against the saved SQLite snapshot and verify the cache and bookmarks survive without requiring another network request.
6. Record the actual commands and results here, and update the spec's implementation status only when both platform checks pass. This platform acceptance is complete; the separate full-CI failure remains open below.

## Recorded validation

- The final iOS simulator smoke returned `OSPREY_INBOX_SMOKE_OK`. The signed Issue Inbox application installed and launched on a physical iPhone 16 and returned the same success marker. Physical-device diagnostics showed eight live public GitHub issues, saved bookmarks, and no application error.
- `make ios-test` passed again for the original counter, including its C ABI fixture, 14 Default/ML simulator goldens, and `OSPREY_IOS_SMOKE_OK`.
- `make android-test` passed C ABI checks, 14 Default/ML language goldens, and the deterministic application workflow. The separate live workflow displayed eight GitHub issues and verified notes/priorities, bookmarks, and SQLite restoration after process restart. The native renderer regression and Android lint passed. ARM64 code was executed; x86-64 code was compiled and linked only.
- Shared Osprey verification passed 29 domain assertions. Compiler verification passed 124 codegen tests and the complete CLI coverage across focused runs, including project imports, annotation checks, staged effects, target capabilities, and the entry/IR regressions found during integration. Strict workspace Clippy and formatting passed.
- Full unchanged `make ci` ran after an existing local Deslop `0.0.0-dev` binary was made available through `PATH`. The duplication report recorded 7,516 duplicated lines out of 79,911 (9.4%), exceeding the configured 5% ceiling. Deslop returned exit 3 and `make ci` exited before its later steps. The earlier default-`PATH` attempt lacked Deslop, but the final blocker is the measured duplication failure. No gate or threshold was changed.

Success markers, live state snapshots, and screenshots live under the ignored platform build directories. Device identifiers and signing credentials are not part of the tracked validation record.

The later Markdown extension passed the expanded 29-case shared suite and native simulator/emulator checks. Its signed iPhone update is installed; iOS refused its launch while the phone was locked. The earlier physical-device smoke and live-data verification remain recorded separately.

## Completion checklist

- [x] Compile the same Osprey application and UI for iOS and Android.
- [x] Verify the scalar boundaries, native services, and supported language goldens.
- [x] Pass deterministic application workflows on both platforms, including annotations and cache recovery.
- [x] Display actual GitHub responses and verify native interaction and persistence.
- [x] Install, launch, and run the full smoke on a signed physical iPhone build.
- [x] Pass shared domain tests, compiler checks, native renderer regression, lint, Clippy, and formatting.
- [ ] Run the updated Markdown smoke on the physical iPhone after it is unlocked.
- [ ] Resolve the duplication gate failure and pass the full unchanged `make ci`; broad repository deduplication is outside this application's delivery.

## Verification requirements

Host tests exercise the actual C ABI, not a rewritten copy of the Osprey update function. Shared application assertions cover request IDs, repository validation, malformed JSON/cache handling, case-insensitive matching, bookmarks, detail navigation, note/priority persistence, and failed refreshes retaining the previous issues. Native SQL checks include scalar binding, quoting/Unicode preservation, statement rejection, and database reopen durability.

Live HTTP is identified separately from deterministic assertions so a network outage cannot masquerade as an offline domain regression or make a stub look like a real integration. Android exposes `--live-smoke` separately from `--smoke`. The iOS `--inbox-diagnostics` flag writes the actual latest envelope to the app sandbox; screenshots complement the state assertions.

## Delivery boundaries

The initial application reads one bounded page of public GitHub issues, has no authentication or background synchronization, and persists one versioned snapshot. Search/filter state is transient. Native hosts must remain generic: additional application behavior belongs in the shared Osprey modules and emitted UI tree. Changes to target capabilities require their own implementation and tests. Follow-on features must retain the same host boundary and cannot conceal unsupported compiler behavior behind native reimplementations.
