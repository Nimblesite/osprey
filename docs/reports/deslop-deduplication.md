# Duplication reduction [MAINTENANCE-DEDUPLICATION]

Target: at most 3% duplication, preserving behavior and assertions. Follow the
[Deslop agent guide](https://deslop.live/docs/for-ai/) and inspect actual pairs.

## Checklist [MAINTENANCE-DEDUPLICATION-CHECKLIST]

- [x] Read the guide, repository instructions, Makefile entry points and dedup skill.
- [x] Establish the editor's Deslop 0.40.1 baseline: 12,333 / 263,549 lines, 4.679585%.
- [x] Recover the bundled MCP; verify its root and read tool schemas and report schema.
- [x] Try permitted pi providers in order: Kimi monthly quota, both GLM time-window quotas and DeepSeek balance prevent delegation.
- [x] Exclude `Book/**` at the maintainer's request; retain the 3% gate and other settings.
- [x] Share native session transitions, protocol envelopes and math decoding failures.
- [x] Share host setup and streamed argument projection; remove forwarding lambdas.
- [x] Move governance's existing Result continuation to shared `Checked`; reuse it in package-manager boundaries.
- [ ] Verify subsequent changes with affected use-case tests and static analysis.
- [ ] Reach and verify 3% with Deslop 0.40.1.
- [ ] Complete repository consolidation, cleanup and push.

## Evidence [MAINTENANCE-DEDUPLICATION-EVIDENCE]

`make lint` passed after the first native changes. It uses pinned Deslop 0.38.0;
its duplication verdict is not comparable with the editor's 0.40.1 results.
All percentages here use 0.40.1 with unchanged embedding policy and clone kinds.

First native batch: 12,209 / 263,501 lines, 4.633379%. After excluding the Book:
12,101 / 263,034 lines, 4.600546%, with zero Book occurrences. This measurement
preceded the package-manager batch.

Existing native/app suites passed: native 78, synth 5, casting 6, Flock 17,
signal-flow 8, storage 20, painting 7. Native passed before and after each native
refactor. Manager CLI passed 98 tests before/after; manifest passed 19 before/after.

Governance does not compile on the original source either: `cannot access field
'packages' on non-struct type Result<...>` and unproven dynamic callable effect
provenance. Temporarily restoring its three original modules reproduced both
errors; refactored source was restored afterward. No assertion was weakened.

## Triage [MAINTENANCE-DEDUPLICATION-TRIAGE]

Highest-mass logo findings are generated coordinate data from
`examples/gpui/painting/reference/logo.py`; preserve values and precision.
Keep independently meaningful teaching/rejection cases, already-factored thin
wrappers and unrelated structural similarities rather than gaming the metric.

The baseline reports 12 unparseable ML files, including native runtime/stream,
three workbench modules and bounded-discipline modules. The percentage does not
prove those files free of duplication. Do not conceal them with exclusions.
