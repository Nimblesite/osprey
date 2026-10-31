# Plan 0043: workbench lessons

> **STOP — NOT YET IN FORCE.** This plan was rewritten on 2026-10-31 for the `.lean`-only direction of [PROOF-SOURCE](../specs/language/0049-DependentTypesAndProofs.md#proof-source-proof-source) while the `feat/type-disciplines` PR is still open. **Any agent working on that PR ignores this rewrite until the PR has merged.** Do not delete, rename, rewrite or stop maintaining any code, test or lesson because of this text before then; the previous revision in git history is the plan for that PR. After the merge, [plan 0044](0044-lean-source-compatibility.md) phase 3 runs first and this plan resumes behind it.

Contract: [spec 0052](../specs/applications/0052-WorkbenchLessons.md). Status: **lesson reader, tutor and Learn mode exist against the deleted format 1; plan 0044 phase 3 moves them to format 2 (`.lean` + extension JSON).** Lessons are a mode of the workbench of [plan 0042](0042-proof-workbench.md); Lean and the kernel judge every answer through the same `Lean` capability and `Verdicts` the workbench uses.

## Checklist

A box is ticked only when its test exists and passes.

### Phase 1 — lessons as data, judged by Lean and the kernel
- [ ] `src/lesson.ospml`: `Lesson`, `Block` and `Challenge` types and `Lessons::read` over format 2, every refusal tested; `decl` references resolved against the lesson's `.lean`
- [ ] `src/judge.ospml`: `Judge::prove` builds the `.lean` document and issues the check; `Judge::state` elaborates and asks `Reduce::defeq`; `Judge::judge` checks the named declaration; each compares a ticketed completion
- [ ] `src/tutor.ospml`: attempts, hints, reveal, clearing on edit, the completed set
- [ ] Framework: `Maths::decode`; `Image` and `Video` with encodings; native README

### Phase 2 — the curriculum
- [ ] `lessons/catalogue.json`, seven `<slug>.lean` files and seven `<slug>.json` extensions
- [ ] The integrity suite of [LESSONS-INTEGRITY] over every shipped lesson, toolchain present
- [ ] Three films (`what-is-a-proof`, `what-is-a-solver`, `navier-stokes`) with posters and captions under the [LESSONS-MEDIA] budget; stills for image blocks

### Phase 3 — Learn mode in the application
- [ ] `Shelf`, `Tutor` and `Achieved`; the messages of [LESSONS-STORE]; the `catalogue`, `lesson` and `challenge:<lesson>/<id>` regions
- [ ] Loading through `ReadTexts`; progress through `StoreRead` and `StoreWrite`
- [ ] Fixture tests through `start`/`dispatch`: per-challenge region refresh, every submit result, stale completion dropped, progress round trip, **Open in the workbench** producing the `.lean` document

### Phase 4 — hosts
- [ ] The renderer draws `Image` and `Video`; the VS Code E2E opens `what-is-a-proof` and answers its first challenge
- [ ] Browser and Apple hosts, with plan 0042 phase 3

## Pi delegation

Under the AGENTS.md rules: the reader's refusal cases, the lesson prose, the `.lean` files and their challenges, the films and the `Image`/`Video` renderer work go to pi, each against a skeleton and test written here first. The judge, the tutor transitions and the integrity case are written and reviewed here. A solution the checker refuses, a weakened test, a framework contract bent to fit a test, any proof text in a JSON file, or any claim in the Navier–Stokes lesson beyond [LESSONS-CURRICULUM] is thrown out.

## What this plan never does

- Store an answer key the checker does not confirm, or mark an answer right without a `Checked` completion.
- Put proof text, a formula in a non-Lean notation, TeX, HTML or MathML in a lesson JSON.
- Gate a lesson behind another; prerequisites are advice.
- Autoplay a film, ship one without captions, or say the Navier–Stokes problem is solved.
