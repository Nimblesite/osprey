# Plan 0030 — Shared Osprey reactive mobile application

**Subsystem:** `examples/mobile/inbox`, iOS Swift host, Android Kotlin/JNI host, and native C ABI targets.

**Status:** implementation in progress; integrated platform and live-network verification is pending.

**Contract:** [Reactive Mobile Applications](../specs/0040-ReactiveMobileApplications.md), [iOS Target](../specs/0038-iOSTarget.md), and [Android Target](../specs/0039-AndroidTarget.md).

## Outcome

Deliver the same usable GitHub Issue Inbox on iPhone and Android. Osprey owns the state transitions, API decoding, SQL commands, filtering, bookmarks, issue details, notes/priorities, labels, and UI layout. Generic native hosts render that UI, execute platform commands, and return completion events. The app uses an event/update pattern supported by today's compiler; it does not depend on a dedicated reactive compiler feature or resumable effects.

## Work and evidence

| Work | Implementation | Acceptance evidence | Status |
| --- | --- | --- | --- |
| Shared state and update rules | `inbox/src/model.ospml`, `update.ospml`, `annotations.ospml` | Initial load, search, saved filter, detail navigation, annotations, request ordering, visible errors | Implemented; integrated assertions pending |
| GitHub input and persistence | `github.ospml`, `storage.ospml` | Pull-request exclusion, invalid-response rejection, bound SQL, cache reopen | Implemented; integrated assertions pending |
| Shared screen | `view.ospml`, `ui.ospml` | Both platforms display the same state and emit equivalent events | Implemented; visual verification pending |
| Scalar application boundary | `main.osp`, `app.ospml`, and generated C headers | Start/dispatch envelope crosses both native bridges | Final builds pending |
| Swift host | `ios/IssueInbox/` | Real SQLite, bounded HTTPS, main-thread dispatch, diagnostics | Isolated native SQL/transport checks passed; app smoke pending |
| Kotlin/JNI host | `android/app/src/main/` | Real SQLite, bounded HTTPS, UI-thread dispatch | Build and app smoke pending |
| Delivery | Platform launch scripts and README | Repeatable simulator/emulator and signed-device launch | Final command verification pending |

## Completion sequence

1. Finish compilation of the shared project and reject unsupported target features before LLVM emission.
2. Build the iOS and Android libraries and native applications from the same project.
3. Run deterministic smoke assertions on both platforms using isolated databases and canned HTTP completions, including Android's process-restart phase. A stale marker cannot pass.
4. Fetch live public GitHub issues and inspect the Osprey envelope and native screen. Confirm pull requests are excluded and HTTP errors are visible.
5. Relaunch against the saved SQLite snapshot and verify the cache and bookmarks survive without requiring another network request.
6. Record the actual commands and results here, and update the spec's implementation status only when both platform checks pass.

## Verification requirements

Host tests exercise the actual C ABI, not a rewritten copy of the Osprey update function. Shared application assertions cover request IDs, repository validation, malformed JSON/cache handling, case-insensitive matching, bookmarks, detail navigation, note/priority persistence, and failed refreshes retaining the previous issues. Native SQL checks include scalar binding, quoting/Unicode preservation, statement rejection, and database reopen durability.

Live HTTP is identified separately from deterministic assertions so a network outage cannot masquerade as an offline domain regression or make a stub look like a real integration. Android exposes `--live-smoke` separately from `--smoke`. The iOS `--inbox-diagnostics` flag writes the actual latest envelope to the app sandbox; screenshots complement the state assertions.

## Delivery boundaries

The initial application reads one bounded page of public GitHub issues, has no authentication or background synchronization, and persists one versioned snapshot. Search/filter state is transient. Native hosts must remain generic: additional application behavior belongs in the shared Osprey modules and emitted UI tree. Changes to target capabilities require their own implementation and tests. Follow-on features must retain the same host boundary and cannot conceal unsupported compiler behavior behind native reimplementations.
