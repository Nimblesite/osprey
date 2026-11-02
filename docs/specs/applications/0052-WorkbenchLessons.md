# Workbench Lessons

> **STOP — NOT YET IN FORCE.** This document was rewritten on 2026-10-31 for the `.lean`-only direction of [PROOF-SOURCE](../language/0049-DependentTypesAndProofs.md#proof-source-proof-source) while the `feat/type-disciplines` PR is still open. **Any agent working on that PR ignores this rewrite until the PR has merged.** Do not delete, rename, rewrite or stop maintaining any code, test, lesson or screen because of this text before then; the previous revision in git history is the contract for that PR. After the merge, [plan 0044](../../plans/0044-lean-source-compatibility.md) is the order of work.

**Status:** rewritten for `.lean`; delivery under [plan 0044](../../plans/0044-lean-source-compatibility.md) phase 3, then [plan 0043](../../plans/0043-workbench-lessons.md). Lessons are a mode of the [proof workbench](0051-ProofWorkbench.md): the same application, the same store, the same checking services. They add one extension format, two framework components (`Image` and `Video`) and three regions.

## Aim [LESSONS-AIM]

Learn mode teaches people to write Lean proofs in the workbench, from the first questions — *what is a proof? what is a solution? what is a solver?* — up to reading a research problem, the Navier–Stokes problem, in Lean's terms. A lesson is never only something to read or watch: each asks the learner to state a claim, prove it, or decide whether a proof holds, in Lean. On **Submit** the same services that fill a badge in the workbench ([WORKBENCH-CHECK](0051-ProofWorkbench.md#checking-workbench-check), [WORKBENCH-VERDICT](0051-ProofWorkbench.md#verdict-workbench-verdict)) judge the answer. No lesson stores an answer key the checker does not confirm.

## The extension format [LESSONS-EXTENSION]

A lesson is two files. `lessons/<slug>.lean` is an ordinary Lean file holding every statement, solution and shown proof the lesson uses, as named declarations; it checks under `osprey prove check` like any other file and contains nothing a lesson-reader needs to understand. `lessons/<slug>.json` is the extension: prose, media, prompts, hints and challenges, each challenge pointing at declarations in the `.lean` file by name. The extension never contains proof text, a formula in any notation other than Lean's, or anything a `.lean` file could hold instead. The deleted sentence-step and formula notations of [PROOF-SOURCE-DELETED] may not appear in any lesson.

## Mode [LESSONS-MODE]

**Examples** and **Play walkthrough** open the lesson catalogue, held as `Learning` in the `Layout` slice. The lesson surface replaces the editor and goal columns with:

| Region | Shows | Reads |
| --- | --- | --- |
| `catalogue` | every lesson in stage order: title, summary, minutes, a tick when every challenge is right; prerequisites are shown as advice, never enforced | `Shelf`, `Achieved` |
| `lesson` | the open lesson as one scrolling page: its blocks in order and a slot per challenge | `Shelf` |
| `challenge:<lesson>/<id>` | one challenge's answer, button and feedback, keyed so that typing re-sends only that challenge | `ChallengeState` |

**Open in the workbench** on a challenge opens a new `.lean` document containing the lesson's preamble, the challenge's statement and the learner's answer, so a lesson leads into ordinary work. Switching modes preserves the document, the open lesson and the answers.

## Files [LESSONS-FILES]

| Path | Holds |
| --- | --- |
| `apps/workbench/lessons/catalogue.json` | the ordered list of lesson slugs and the stage names |
| `apps/workbench/lessons/<slug>.lean` | the lesson's declarations |
| `apps/workbench/lessons/<slug>.json` | the extension |
| `apps/workbench/lessons/media/<slug>/` | images, videos, posters and captions |

At start the application reads `catalogue.json` with `ReadTexts` into host-held slice `lessons:catalogue`, then every lesson's two files into `lessons:texts`; each read stores `{ files: [{ path, text, error }] }`. A lesson that fails to read is listed with its fault and cannot be opened. Hosts contain no lesson judgement or proof logic. Media paths are relative to `apps/workbench/lessons/`, must start with `media/<slug>/`, and may not contain `..`, a backslash or a leading `/`.

## The lesson format [LESSONS-FORMAT]

```json
{
  "format": 2,
  "slug": "and-swap",
  "title": "Swapping a conjunction",
  "summary": "Take the pair apart, then build it the other way round.",
  "stage": 2,
  "minutes": 8,
  "after": ["what-is-a-proof"],
  "blocks": [
    { "kind": "prose", "text": "..." },
    { "kind": "lean", "decl": "and_swap" },
    { "kind": "lean", "source": "p ∧ q → q ∧ p" },
    { "kind": "math", "math": { "kind": "op", "symbol": "=", "left": ..., "right": ... }, "spoken": "..." },
    { "kind": "image", "source": "media/and-swap/pair.jpg", "alt": "...", "width": 1920, "height": 1080 },
    { "kind": "video", "source": "media/and-swap/film.mp4", "poster": "media/and-swap/poster.jpg", "captions": "media/and-swap/film.en.vtt", "alt": "...", "seconds": 40 },
    { "kind": "challenge", "challenge": { ... } }
  ]
}
```

| Field | Rule |
| --- | --- |
| `format` | `2`. `1` and anything else is refused with "this lesson needs a newer workbench". |
| `slug` | lowercase letters, digits and hyphens; equal to both file names and unique in the catalogue |
| `title`, `summary` | non-empty |
| `stage` | `1` first steps, `2` proving, `3` research |
| `minutes` | positive |
| `after` | slugs this lesson builds on; each must be in the catalogue |
| `blocks` | non-empty; at least one block is a challenge |

Blocks:

| `kind` | Fields | Drawn as |
| --- | --- | --- |
| `prose` | `text`: paragraphs separated by a blank line; no markup | body text at reading measure |
| `lean` | `decl`: a declaration name in the lesson's `.lean`; or `source`: a Lean term, elaborated in the lesson's `.lean` context | the statement or term typeset through [WORKBENCH-MATH], with the Lean source shown beneath on request; refused if Lean rejects it |
| `math` | `math`: a `ui::Math` tree; `spoken`: what assistive technology reads | display mathematics beyond what a `.lean` file states (physics, integrals) |
| `image` | `source`, `alt` (non-empty), `width`, `height` | the `Image` component |
| `video` | `source`, `poster`, `captions` (WebVTT), `alt`, `seconds` | the `Video` component; never autoplays |
| `challenge` | `challenge`: one of [LESSONS-CHALLENGES] | the `challenge:<lesson>/<id>` region |

`Maths::decode` is the inverse of `Maths::encode` and refuses an unknown `kind`. A lesson never contains TeX, HTML or MathML.

## Challenges [LESSONS-CHALLENGES]

Every challenge has an `id` (unique within its lesson), a `prompt`, zero or more `hints`, and one `kind`. Three kinds, all judged by Lean and the kernel.

### Prove [LESSONS-PROVE]

```json
{ "kind": "prove", "id": "swap", "prompt": "Prove that the order of a conjunction does not matter.",
  "statement": "and_swap", "given": ["intro h"], "solution": "and_swap_solution",
  "hints": ["Take the conjunction apart first: cases h with hp hq.", "Then build the new one: constructor."] }
```

`statement` names a declaration in the lesson's `.lean` whose body is `by sorry`; the learner writes tactic lines into a marked editor under the locked `given` lines. On **Submit** the workbench builds a `.lean` document from the lesson's preamble, the statement's signature, `by`, the given lines and the answer, and checks it through [WORKBENCH-CHECK]. The answer is right exactly when the verdict is `Checked`. `solution` names a declaration in the `.lean` with the same signature and a complete proof; **Show a solution** copies its tactic lines into the answer.

### State [LESSONS-STATE]

```json
{ "kind": "state", "id": "rain", "prompt": "Write: if it rains, the ground is wet.",
  "variables": ["rain", "wet"], "expected": "rain_wet",
  "hints": ["\"If … then …\" is an arrow."] }
```

The learner writes a Lean proposition over the `variables` (each a `Prop` in the lesson's `.lean`). On **Submit** the workbench elaborates it in the lesson context; a Lean error is shown as one, and a name outside `variables` is refused with the variables listed. Otherwise the kernel decides whether the answer and the declaration `expected` are **the same proposition by definition** (`Reduce::defeq`): `¬rain` and `rain → False` are both accepted where either is expected; `wet ∧ rain` is not accepted for `rain ∧ wet`, and the feedback says so and points at the prove challenge that shows it.

A state challenge shows a keypad of its variables, the connectives and parentheses with plain names (Implies for `→`), spelled from the shared notation table. A key inserts at the attempt's bounded UTF-8 byte caret, advances the caret and clears stale marking without submitting; carets are independent per challenge; insertion alone never produces a verdict. A keypad insertion advances a per-attempt `focusEpoch`; a retained editor returns focus only for an epoch greater than every epoch it has observed, and editor identity includes the lesson as well as the challenge.

### Judge [LESSONS-JUDGE]

```json
{ "kind": "judge", "id": "wrong-order", "prompt": "Does this proof check?", "proof": "and_swap_wrong" }
```

`proof` names a declaration in the lesson's `.lean`. The learner sees it typeset with its tactic lines and chooses **It checks** or **It fails**. On **Submit** the workbench checks that declaration then and there; the learner is right when the choice matches. The file holds no answer. Feedback shows what Lean and the kernel said, with the refused line underlined.

### Submitting and feedback [LESSONS-FEEDBACK]

**Submit** is enabled when the answer is non-empty (and, for judge, a choice is made). It issues one check with a revision ticket; a completion for an older revision is dropped. The challenge then shows exactly one of:

| Result | When | Shown |
| --- | --- | --- |
| `Correct message` | prove: `Checked`; state: same proposition; judge: choice matches | a "Right" badge and the explanation |
| `Incorrect line message` | prove: `Rejected`; state: different proposition; judge: choice does not match | a "Not yet" badge; the refused line underlined with Lean's or the kernel's message |
| `Unfinished reason` | prove: `Unchecked` (a `sorry`, an unsolved goal); state: Lean cannot elaborate the text; no toolchain | "Not finished yet:" and the reason; does not count as an attempt |

Every `Incorrect` adds one attempt. Hint *n* appears after *n* wrong attempts or on **Show a hint**. A shown solution marks the challenge *seen*, not *right*, until the learner submits it themselves. Editing after `Correct` clears the result.

### Every shipped lesson is checked [LESSONS-INTEGRITY]

The suite refuses the build when any of these fails for any lesson in `apps/workbench/lessons/`:

- the `.json` reads with no fault and its slug, `after` list and challenge ids follow [LESSONS-FORMAT]; every referenced `decl` exists in the `.lean`;
- the `.lean` checks under `osprey prove check` with every `statement` declaration `Unchecked` (its body is `sorry`), every `solution` and `expected` declaration `Checked`, and every judge `proof` `Checked` or `Rejected`, never `Unchecked`;
- every `lean` block with `source` elaborates;
- every media path follows [LESSONS-FILES] and the file exists.

## Progress [LESSONS-PROGRESS]

The `Tutor` slice holds, per challenge, the answer text, the choice, the attempts, the hints shown and the last result, and the set of challenges answered right. Progress is written through host-held slice `lessons:progress` with `StoreWrite` to `lessons/progress.json` after each newly right answer and read with `StoreRead` at start; it holds challenge identifiers and nothing else. A missing file means no progress; a failed write reports that progress was not saved.

## Framework additions [LESSONS-FRAMEWORK]

Lessons draw their pictures and films with `@osprey/native`'s `Media` module ([UI-COMPONENTS-MEDIA](../../../packages/native/README.md#media-ui-components-media)) and read `math` blocks back with `Maths::decode` ([UI-COMPONENTS-MATH](../../../packages/native/README.md#typeset-mathematics-ui-components-math)). Neither module knows about lessons.

## The curriculum [LESSONS-CURRICULUM]

Seven lessons in three stages; each has at least two challenges and at least one prove challenge except where noted. Every lesson teaches Lean's own words (`intro`, `cases … with`, `constructor`, `exact`, `left`, `right`, `absurd`) with a one-line English explanation beside each.

| Stage | Slug | Teaches | Media |
| --- | --- | --- | --- |
| 1 | `what-is-a-proof` | A claim, the steps, the rule behind each step, why a checked proof leaves no room for doubt | film |
| 1 | `what-is-a-solution` | A proposition is a problem and its proof is the solution | image |
| 1 | `what-is-a-solver` | Searching versus checking; why the checker's verdict can be trusted; an honest "not finished" | film |
| 2 | `and-swap` | The first full proof: `intro`, `cases`, `constructor`, `exact` | image |
| 2 | `either-way` | Proof by cases on `∨` | none |
| 2 | `not-and-contradiction` | `¬p` as `p → False`, and `absurd` | none |
| 3 | `navier-stokes` | The equations, what a solution is, classical results, and the OpenAI solution with its Lean formalization | film, image |

**The Navier–Stokes lesson** teaches the [solution published by OpenAI on 8 September 2026](https://openai.com/index/navier-stokes-solution/): smooth forcing can produce a finite-time singularity from smooth fluid at rest while energy stays finite, establishing statements C and D of the Millennium formulation. It teaches the equations (`math` blocks with *u*, *p*, *ν*), what a smooth solution supplies, what was proved before (Leray 1934; global smooth solutions in two dimensions; local in three; small-data results with their hypotheses) and the published result with its hypotheses. Its challenges use `Prop`s named `smooth` and `blowup`: prove `smooth ∨ blowup → ¬blowup → smooth`; state that both cannot hold; judge a "proof" of `smooth` from nothing, which the checker refuses. Credit the published solution. Osprey checks selected supporting exports from `openai/math` and does not check the complete theorem; keep the three levels distinct.

## Films and images [LESSONS-MEDIA]

Films come from the Nimblesite TikTok content factory (`factory film`): 1920×1080 H.264 with AAC, fast start, at most 2 MB and 60 seconds, narrated with English WebVTT captions, with a JPEG poster. Sources live in that repository under `projects/osprey/videos/lesson-<slug>/`; only published files are copied into `media/<slug>/`. Images are JPEG at true size, at most 400 KB. A film's narration says only what its lesson says.

## Store and messages [LESSONS-STORE]

| Slice | Held | Contents |
| --- | --- | --- |
| `lessons:catalogue`, `lessons:texts` | host | bounded file-read results |
| `lessons:progress` | host | saved completed challenge identifiers |
| `Shelf` | session | catalogue summaries and faults, the open lesson's slug and both sources, the focused challenge, loading or save errors |
| `Tutor` | session | per-challenge attempt state with its caret and focus epoch, and the completed set |
| `Achieved` | derived | completed challenge keys |

The action union carries `ToLearn`, `ToWork`, `Open slug`, `Answer id`, `FormulaPlace id`, `Key id text`, `Pick id`, `Submit id`, `Hint id`, `Reveal id` and `Carry id`. Judging is a check request plus a pure comparison of its completion; file and progress operations are bounded host requests.

## Verification [LESSONS-VERIFICATION]

In `apps/workbench/tests/`, each a fixture through the real `start`/`dispatch` where it concerns the application:

- the reader accepts every block and challenge kind and refuses each broken form: unknown `kind`, missing field, bad slug, unsafe media path, unknown `decl`, `format` 1;
- `Maths::decode (Maths::encode t) = t` over a corpus of trees, in `packages/native/tests/`;
- prove: a `Checked` completion is `Correct`; a `Rejected` completion at line 4 is `Incorrect` at line 4 with its message; a `sorry` is `Unfinished`; a stale completion is dropped;
- state: an exact answer, an answer equal by definition (`¬p` for `p → False`), a different but equivalent answer (`Incorrect`), a stray name and an unparsable formula;
- keypad: caret insertion at start, middle, end and clamped; multi-byte offsets; independent attempts; marking invalidation without submission; focus high-water and lesson-qualified identity reset;
- judge: both choices against a `Checked` and a `Rejected` declaration;
- attempts, hints and the solution reveal follow [LESSONS-FEEDBACK]; editing after `Correct` clears it;
- a keystroke in one answer re-sends only that `challenge:<lesson>/<id>` region;
- progress round-trips through `StoreWrite` and `StoreRead`;
- every shipped lesson passes [LESSONS-INTEGRITY] with the toolchain present;
- the `Image` and `Video` encodings, in `packages/native/tests/`;
- no lesson file contains the deleted formats (the grep gate of [WORKBENCH-VERIFICATION]).

Each host's smoke test opens `what-is-a-proof`, plays nothing, answers its first challenge and sees the "Right" badge.
