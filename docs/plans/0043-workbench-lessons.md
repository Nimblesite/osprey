# Plan 0043: workbench lessons

Contract: [spec 0052](../specs/applications/0052-WorkbenchLessons.md). Status: **spec written; phase 1 in progress.** Lessons are a mode of the proof workbench of [plan 0042](0042-proof-workbench.md), and the kernel judges every answer. They need the in-process kernel verdict (`apps/workbench/src/verdict.ospml` and `kernel.ospml`, which already exist), the `Math`, `Editor` and `Board` components of plan 0042 phase 2, and nothing from the compiler.

An agent picking this up reads spec 0052, then spec 0051's [WORKBENCH-VERDICT](../specs/applications/0051-ProofWorkbench.md#verdict-workbench-verdict) and [WORKBENCH-STEPS](../specs/applications/0051-ProofWorkbench.md#steps-workbench-steps), and the [`@osprey/native` README](../../packages/native/README.md).

## Checklist

A box is ticked only when its test exists and passes.

### Phase 1 — lessons as data, judged by the kernel
- [ ] `src/lesson.ospml`: the `Lesson`, `Block` and `Challenge` types and `Lessons::read` over [LESSONS-FORMAT], with every refusal of the reader tested
- [ ] `src/judge.ospml`: `Judge::prove`, `Judge::state` and `Judge::judge`, each through `Verdicts::decide` or the kernel's `Reduce::defeq`
- [ ] `src/tutor.ospml`: attempts, hints, reveal, clearing on edit and the completed set ([LESSONS-FEEDBACK], [LESSONS-PROGRESS])
- [ ] Framework: `Maths::decode` with the round-trip corpus; the `Image` and `Video` components and their encodings; native README

### Phase 2 — the curriculum
- [ ] `lessons/catalogue.json` and the seven lessons of [LESSONS-CURRICULUM]
- [ ] The integrity suite of [LESSONS-INTEGRITY] over every shipped lesson
- [ ] Three films (`what-is-a-proof`, `what-is-a-solver`, `navier-stokes`) from the TikTok factory, with posters and captions, under the budget of [LESSONS-MEDIA]; stills for the image blocks

### Phase 3 — Learn mode in the application
- [ ] The `Lessons` and `Tutor` slices, the messages of [LESSONS-STORE] and the `catalogue`, `lesson` and `challenge:<id>` regions in `src/app.ospml`
- [ ] Loading through `ReadTexts`; progress through `StoreRead` and `StoreWrite`
- [ ] Fixture tests through `start`/`dispatch`: the per-challenge region refresh, the submit results, progress round trip, recognised text into the focused answer, **Open in the workbench**

### Phase 4 — hosts
- [ ] The renderer draws `Image` and `Video` (poster first, captions on, no autoplay); the VS Code host's E2E test opens `what-is-a-proof` and answers its first challenge
- [ ] The browser and Apple hosts, with plan 0042 phase 3

## Phase 1 — lessons as data, judged by the kernel [LESSONS-FORMAT] [LESSONS-CHALLENGES] [LESSONS-FEEDBACK]

1. `src/lesson.ospml`. The types are a direct transcription of the format. `Lessons::read : string -> Result<Lesson, Error>`, with each refusal naming the JSON path (`blocks[3].challenge.statement`) and the rule broken. A `math` block goes through `ui::Maths::decode`; a `formula` block through `Formulas::read`. Media paths are checked against [LESSONS-FILES] here, so no later layer sees an unsafe path.
2. `src/judge.ospml`. `Judge::prove (challenge, answer)` builds the document text, reads it and calls `Verdicts::decide`; `Checked` is `Right`, `Rejected` is `Wrong` with the step index mapped to the answer line (the given steps come first), and `Unchecked` is `Unfinished`. `Judge::state (challenge, answer)` reads the formula, refuses stray atoms, closes both formulas as kernel propositions over the variables, sort-checks the answer and asks `Reduce::defeq` in the prelude environment. `Judge::judge (challenge, choice)` asks `Verdicts::decide` on the shown proof and compares.
3. `src/tutor.ospml`. The per-challenge record and pure transitions: `answer`, `pick`, `submit`, `hint`, `reveal`. These are the only places attempts change.
4. Framework. `Maths::decode` in `packages/native/src/math.ospml`; the `Image` and `Video` node variants, `Ui::image` and `Ui::video`, and their encodings; native tests and README sections under [UI-COMPONENTS].

Exit: every case in [LESSONS-VERIFICATION] that does not need the application passes; the suite stays at 90% line coverage.

## Phase 2 — the curriculum [LESSONS-CURRICULUM] [LESSONS-MEDIA] [LESSONS-INTEGRITY]

1. Write the seven lessons. Prose is plain, short and exact. Every prove challenge carries a `solution`, and every lesson has a challenge.
2. The integrity case reads every file in `apps/workbench/lessons/` with `readFile` and asserts the whole of [LESSONS-INTEGRITY]. It runs in the ordinary suite, so a broken lesson fails `make ci`.
3. Films. In the TikTok repository, under `projects/osprey/videos/lesson-<slug>/`, on the model of `logic-in-flight`: narration text taken from the lesson, the Osprey website's tokens and fonts, typeset mathematics as the lesson shows it. Run `factory film osprey/lesson-<slug> --publish`, inspect the frames, and copy only the published mp4, poster and captions into `apps/workbench/lessons/media/<slug>/`. Encode for the 2 MB budget.

Exit: the integrity case passes over seven lessons; three films, their posters and captions are in place; each film's narration has been read against its lesson.

## Phase 3 — Learn mode [LESSONS-MODE] [LESSONS-STORE] [LESSONS-PROGRESS]

Lands with, or after, plan 0042 phase 2's `src/app.ospml`. It adds the two slices, the messages and the three regions, and changes no existing region except the root's mode segment.

Exit: the fixture cases of [LESSONS-VERIFICATION] pass through the real `start`/`dispatch`.

## Phase 4 — hosts [LESSONS-FRAMEWORK]

With plan 0042 phase 3. The renderer gains `Image` and `Video`, and each host's smoke test answers a challenge.

## Pi delegation

Under the AGENTS.md rules: the reader's refusal cases, the lesson prose and its challenges, the films and the `Image`/`Video` renderer work go to pi, each against a skeleton and test written here first. The judge, the tutor transitions and the integrity case are written and reviewed here. Every pi result is reviewed line by line before it stays: a solution the kernel refuses, a weakened test, a framework contract bent to fit a test, or any claim in the Navier–Stokes lesson beyond what [LESSONS-CURRICULUM] allows is thrown out.

## What this plan never does

- Store an answer key the kernel does not confirm, or mark an answer right without the kernel.
- Gate a lesson behind another; prerequisites are advice.
- Autoplay a film, ship one without captions, or say the Navier–Stokes problem is solved.
- Put TeX, HTML or MathML in a lesson.
