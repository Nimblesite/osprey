# Workbench Lessons

**Status:** specification written; implementation in progress under [plan 0043](../../plans/0043-workbench-lessons.md). Lessons are a mode of the [proof workbench](0051-ProofWorkbench.md): the same application, the same store, the same kernel. They add one content format, two framework components (`Image` and `Video`) and two regions.

## Aim [LESSONS-AIM]

The workbench is the interface to Osprey's proof kernel for people who will never write code. Learn mode teaches those people to use it. Lessons run from the first questions anyone asks — *what is a proof? what is a solution? what is a solver?* — up to reading a research problem, the Navier–Stokes problem, in the terms the workbench uses.

A lesson is never only something to read or watch. Each one asks the learner to do the work: state a claim, prove it, or decide whether a proof holds. The learner presses **Submit**, and **the kernel of [spec 0049](../language/0049-DependentTypesAndProofs.md) judges the answer**, the same kernel and the same path that fills a badge in the workbench ([WORKBENCH-VERDICT](0051-ProofWorkbench.md#verdict-workbench-verdict)). The lesson tells the learner whether they are right and, when they are not, which step the kernel refused and why. No lesson stores an answer key that the kernel does not confirm: an answer is right because the kernel accepts it, not because it matches text in a file.

## Mode [LESSONS-MODE]

The supported working-document workflow uses **Proof** and **Read**. The freehand board, drawing tools and transcription preview are being removed from the Workbench; they are not part of the lesson contract. The shared native SDK drawing APIs remain independent of this application change. **Examples** and **Play walkthrough** open the lesson catalogue, held as `Learning` in the `Layout` slice ([WORKBENCH-STORE](0051-ProofWorkbench.md#store-workbench-store)). The lesson surface replaces the document and goal columns with catalogue and lesson regions, plus one keyed region per challenge:

| Region | Shows | Reads |
| --- | --- | --- |
| `catalogue` | every lesson in stage order: title, summary, minutes, a tick when every challenge in it is right, and an unrestricted list (the reader validates prerequisites, but the catalogue does not display or enforce them) | `Shelf`, `Achieved` |
| `lesson` | the open lesson as one scrolling page: its blocks in order and a slot for each inline challenge | `Shelf` |
| `challenge:<lesson>/<id>` | one challenge's answer, button and feedback, keyed so that typing in one answer re-sends only that challenge | `ChallengeState`, containing that challenge and its own `Attempt` |

**Open in the workbench** on a challenge copies its statement and the learner's answer into the Work document as a new declaration, so a lesson leads into ordinary work.

Switching between the working document and lessons preserves the document, the open lesson and the answers in application state. Scroll-position retention is a host acceptance requirement; the application does not serialize scroll offsets.

## Files [LESSONS-FILES]

Lessons ship with the application as data, not code:

| Path | Holds |
| --- | --- |
| `apps/workbench/lessons/catalogue.json` | the ordered list of lesson slugs and the stage names |
| `apps/workbench/lessons/<slug>.json` | one lesson |
| `apps/workbench/lessons/media/<slug>/` | that lesson's images, videos, posters and captions |

At start the application asks the host for `lessons/catalogue.json` with `ReadTexts` ([UI-EFFECTS](../../../packages/native/README.md)) into host-held slice `lessons:catalogue`. Once the catalogue is read, a second request reads every lesson it names into `lessons:texts`. Each read stores a JSON object with a `files` array containing `path`, `text` and `error` for each file. A lesson that fails to read is listed with its fault and cannot be opened; the others are unaffected. The session retains catalogue summaries and only the open lesson's source, rather than every lesson's parsed content.

The host supplies its current text slices as the `frame.slices` object on every start and dispatch frame. `App::respond` handles `ui::Slices.fetch` from that object; completions carry request results and never substitute for the host-held lesson text. Browser and native hosts invoke the same application boundary and execute its requests. They contain no lesson judgement or proof logic.

Every lesson file is UTF-8 JSON. Media paths are relative to `apps/workbench/lessons/`, must start with `media/<slug>/`, and may not contain `..`, a backslash or a leading `/`. The reader refuses a lesson that breaks this rule, so a lesson can never point a host at a file outside its own folder.

## The lesson format [LESSONS-FORMAT]

```json
{
  "format": 1,
  "slug": "what-is-a-proof",
  "title": "What is a proof?",
  "summary": "A proof is a chain of steps, each one allowed by a rule, ending at the claim.",
  "stage": 1,
  "minutes": 5,
  "after": [],
  "blocks": [
    { "kind": "prose", "text": "..." },
    { "kind": "formula", "formula": "p ∧ q → q ∧ p" },
    { "kind": "math", "math": { "kind": "op", "symbol": "=", "left": ..., "right": ... }, "spoken": "..." },
    { "kind": "image", "source": "media/what-is-a-proof/chain.jpg", "alt": "...", "width": 1920, "height": 1080 },
    { "kind": "video", "source": "media/what-is-a-proof/film.mp4", "poster": "media/what-is-a-proof/poster.jpg", "captions": "media/what-is-a-proof/film.en.vtt", "alt": "...", "seconds": 40 },
    { "kind": "challenge", "challenge": { ... } }
  ]
}
```

| Field | Rule |
| --- | --- |
| `format` | `1`. Any other value is refused with "this lesson needs a newer workbench". |
| `slug` | lowercase letters, digits and hyphens; equal to the file name and unique in the catalogue |
| `title`, `summary` | non-empty |
| `stage` | `1` first steps, `2` proving, `3` research; the catalogue groups by it |
| `minutes` | positive; the learner's expected time including the challenges |
| `after` | slugs this lesson builds on; each must be in the catalogue |
| `blocks` | non-empty; at least one block is a challenge |

Blocks:

| `kind` | Fields | Drawn as |
| --- | --- | --- |
| `prose` | `text`: paragraphs separated by a blank line; no markup | body text at reading measure |
| `formula` | `formula`: workbench notation read by `Formulas::read` ([WORKBENCH-FORMULA]) | display mathematics through `Formulas::typeset`; the lesson is refused if it does not read |
| `math` | `math`: a `ui::Math` tree in its protocol form ([UI-COMPONENTS-MATH]); `spoken`: the text assistive technology reads | display mathematics, for anything beyond the workbench's formulas (equations of physics, sums, integrals) |
| `image` | `source`, `alt` (non-empty), `width`, `height` | the `Image` component |
| `video` | `source`, `poster`, `captions` (WebVTT), `alt`, `seconds` | the `Video` component; never autoplays |
| `challenge` | `challenge`: one of [LESSONS-CHALLENGES] | the `challenge:<lesson>/<id>` region |

The `math` tree is decoded by `Maths::decode`, the inverse of `Maths::encode`: `decode (encode t) = t` for every tree, and an unknown `kind` or a missing child refuses the lesson. A lesson never contains TeX, HTML or MathML.

## Challenges [LESSONS-CHALLENGES]

Every challenge has an `id` (unique within its lesson), a `prompt` (what the learner is asked to do, in prose), zero or more `hints`, and one `kind`. Three kinds exist, and the kernel judges all three.

### Prove [LESSONS-PROVE]

```json
{ "kind": "prove", "id": "swap", "prompt": "Prove that the order of a conjunction does not matter.",
  "statement": "Theorem and_swap. For all p q, p ∧ q → q ∧ p.",
  "given": ["Assume h : p ∧ q."],
  "solution": ["By cases on h, as hp and hq.", "It suffices to show q and p.", "From hq.", "From hp."],
  "hints": ["Take the conjunction apart first.", "Then build the new one, a side at a time."] }
```

The learner writes proof steps, one sentence per line, in the step language of [WORKBENCH-STEPS](0051-ProofWorkbench.md#steps-workbench-steps), into a marked editor. `given` steps are shown above the answer, locked; they let a lesson ask for the middle of a proof. On **Submit** the workbench builds the document `statement`, `Proof.`, the given steps and the answer, reads it with `Statements::document` and `Steps::read`, and asks `Verdicts::decide` ([WORKBENCH-VERDICT]). The answer is right exactly when the verdict is `Checked`.

### State [LESSONS-STATE]

```json
{ "kind": "state", "id": "rain", "prompt": "Write: if it rains, the ground is wet.",
  "variables": ["rain", "wet"], "expected": "rain → wet",
  "hints": ["\"If … then …\" is an arrow."] }
```

The learner writes a formula. On **Submit** the workbench reads it with `Formulas::read`; a fault is shown as one, and an atom outside `variables` is refused with the variables listed. Otherwise both formulas become kernel propositions over `variables` ([WORKBENCH-VERDICT]); the kernel checks that the answer is a proposition (`Infer::sort`) and then decides whether the answer and `expected` are **the same proposition by definition** (`Reduce::defeq` in the prelude environment). So `¬rain` and `rain → ⊥` are both accepted where either is expected, because the kernel unfolds `Not`; `wet ∧ rain` is not accepted for `rain ∧ wet`, because the two are different propositions that happen to be equivalent, and the feedback says so and points to the prove challenge that shows it.

Every state challenge shows a formula keypad below Your formula and before feedback or submission controls. It offers the challenge's declared variables, logical connectives and parentheses, with plain-language names including Implies for →. Spellings come from the shared notation table. Short paired rows fit narrow lesson panes; ordinary text entry remains available. The shared Osprey application owns the keypad, answer and caret state for native and VSIX.

Activating a key inserts at the attempt's bounded UTF-8 byte caret, preserves surrounding text, advances the caret and clears stale marking without submitting or incrementing attempts. Carets are independent per challenge. Saved attempts without a cursor restore at the end of their answer. Insertion alone never produces a kernel verdict. The typed place event carries a caret, not a selection span. Selection replacement and the complete lesson interaction still require actual native/VSIX acceptance; the static formula-keypad design handoff is a proposal, not runtime acceptance.

A keypad insertion authors a per-attempt `focusEpoch` advance. A retained editor returns focus only for an epoch greater than every epoch it has already observed; an ordinary text patch, a repeated value, a decrease or replay of an earlier high value must not steal focus. Thus 8 → 6 → 8 cannot refocus the same editor. A newly created editor establishes its own baseline without claiming focus immediately.

Editor identity includes the lesson as well as the challenge: entering another lesson must create a distinct retained identity or reset the host editor's baseline, even if the challenge identifiers coincide. A high epoch from lesson A cannot suppress a later advance from lesson B. Native Paragraph tests cover the retained high-water sequence and actual RegionStore transitions A8 → B0 → B1; they do not alone establish the full application transition or VSIX acceptance.

### Judge [LESSONS-JUDGE]

```json
{ "kind": "judge", "id": "wrong-order", "prompt": "Does this proof check?",
  "statement": "Theorem and_swap. For all p q, p ∧ q → q ∧ p.",
  "steps": ["Assume h : p ∧ q.", "By cases on h, as hp and hq.", "It suffices to show q and p.", "From hp.", "From hq."] }
```

The learner is shown a proof, typeset, and chooses **It checks** or **It fails**. On **Submit** the workbench asks the kernel for the verdict of that proof, then and there; the learner is right when their choice matches it. The file holds no answer. The feedback then shows what the kernel said: for a rejected proof, the refused step underlined with the kernel's message and the goal engine's readable mismatch beside it.

### Submitting and feedback [LESSONS-FEEDBACK]

**Submit** is enabled when the answer is non-empty (and, for judge, a choice is made). It runs the kernel in `update`, synchronously; there is no request to wait for and no spinner. The challenge then shows exactly one of:

| Result | When | Shown |
| --- | --- | --- |
| `Correct message` | prove: `Checked`; state: same proposition; judge: choice matches the kernel | a “Right” badge and the task-specific kernel explanation |
| `Incorrect step message` | prove: `Rejected`; state: different proposition or not a proposition; judge: choice does not match | a “Not yet” badge; for prove, the step underlined with the kernel's message and the goal engine's mismatch; for state, both formulas typeset side by side |
| `Unfinished reason` | prove: `Unchecked` (an unknown step, an open goal, steps after the end); state: the formula does not read | "Not finished yet:" and the reason; this does not count as an attempt |

Every `Incorrect` adds one attempt. Hint *n* is shown after *n* wrong attempts, or earlier on **Show a hint**. A prove challenge with a `solution` offers **Show a solution** after three wrong attempts; a shown solution fills the answer, and the challenge is marked *seen* rather than *right* until the learner submits it themselves. Editing the answer after `Correct` clears the result, so the badge always describes the text on screen.

### Every shipped lesson is checked [LESSONS-INTEGRITY]

The suite reads every lesson in `apps/workbench/lessons/` with `readFile` and refuses the build when any of these fails:

- the lesson reads with no fault, and its slug, `after` list and challenge ids follow [LESSONS-FORMAT];
- every `solution` of every prove challenge is `Checked` by the kernel;
- every `expected` of every state challenge reads, uses only its `variables` and is a proposition the kernel accepts;
- every judge challenge's proof is `Checked` or `Rejected` by the kernel, never `Unchecked`, so the question always has an answer;
- every media path follows [LESSONS-FILES] and the file exists.

A lesson author cannot ship a solution the kernel refuses or a question the kernel cannot settle.

## Progress [LESSONS-PROGRESS]

The `Tutor` slice holds, per challenge, the answer text, the choice, the attempts, the hints shown and the last result, and the set of challenges answered right. A lesson is complete when all its challenges are right. Progress is written through host-held slice `lessons:progress` with `StoreWrite` to `lessons/progress.json` after each newly right answer and read into the same slice with `StoreRead` at start; it holds challenge identifiers (`<slug>/<id>`) and nothing else. A missing or unreadable file means no progress, never an error dialog. A failed write must report that progress was not saved.

## Framework additions [LESSONS-FRAMEWORK]

`@osprey/native` gains two content components, usable by any application:

- `Image` (`source`, `alt`, `width`, `height`): a still picture, scaled to fit its width and keeping its aspect ratio. `alt` is required and is what assistive technology reads.
- `Video` (`source`, `poster`, `captions`, `alt`, `seconds`): a film the person starts. It shows the poster and a play control until then, and never autoplays. Captions are on by default, and the length is printed on the poster. Under reduced motion the poster stays until play is pressed, as it always does. The host may refuse a source outside the application's bundled files.

Both encode as protocol nodes `"image"` and `"video"` with the fields above. The renderer draws them with the platform's image and media players. Neither carries a binding: playing a video is the host's business and never reaches `update`. `Maths::decode` is added beside `Maths::encode`.

## The curriculum [LESSONS-CURRICULUM]

The first release ships seven lessons in three stages. Each has at least two challenges and at least one prove challenge, except where noted.

| Stage | Slug | Teaches | Media |
| --- | --- | --- | --- |
| 1 | `what-is-a-proof` | A claim, the steps, the rule behind each step, and why a checked proof leaves no room for doubt | film |
| 1 | `what-is-a-solution` | A problem asks for something; a solution is a thing that meets every condition. A proposition is a problem, and its proof is the solution. | image |
| 1 | `what-is-a-solver` | Searching for a solution versus checking one; the kernel only checks, which is why its verdict can be trusted; an honest "not finished" | film |
| 2 | `and-swap` | The first full proof: assume, take apart, build, finish | image |
| 2 | `either-way` | Proof by cases on "or" | none |
| 2 | `not-and-contradiction` | Negation as "leads to the impossible", and contradiction | none |
| 3 | `navier-stokes` | The Navier–Stokes equations of fluid motion, what a solution of them is, classical results, and the OpenAI solution with Lean formalization | film, image |

**The Navier–Stokes lesson** teaches the [solution published by OpenAI on 8 September 2026](https://openai.com/index/navier-stokes-solution/), with an analytical proof and Lean formalization. Smooth forcing can produce a finite-time singularity from initially smooth fluid at rest while energy remains finite, establishing statements C and D of the Millennium Prize formulation. The lesson teaches:

- the equations, with the velocity *u*, the pressure *p* and the viscosity *ν*, typeset with `math` blocks;
- what a smooth solution supplies: velocity and pressure satisfying the equations throughout its time interval and starting from the prescribed initial flow;
- what is proved: Leray's weak solutions (1934) exist for all time; in two dimensions smooth solutions exist for all time; in three dimensions smooth solutions exist locally in time; global unforced small-data results require their size and regularity hypotheses;
- the published result and its hypotheses, including smooth external forcing; distinguish it from the separate unforced two-dimensional demonstration.

Its challenges work on the logical shape of the problem with propositions named `smooth` and `blowup`:

- prove that "smooth or blowup" together with "not blowup" gives "smooth";
- state that the two outcomes cannot both hold;
- judge a "proof" of `smooth` from no hypotheses at all. The kernel refuses it, and the lesson says why: the workbench requires a proof term matching the explicit assumptions and conclusion; a proposition named `smooth` supplies no fluid analysis.

Credit the published solution explicitly. The lesson exercises propositional reasoning; Osprey checks selected supporting exports from `openai/math`, and does not yet check the complete Navier–Stokes theorem. Keep these three levels distinct.

## Films and images [LESSONS-MEDIA]

The films are made with the content factory of the Nimblesite TikTok repository (`factory film`), in the same pipeline and visual system as the Osprey homepage film. Each film is:

- 1920×1080 H.264 with AAC sound and fast start, at most 2 MB, and at most 60 seconds;
- narrated, with English WebVTT captions generated from the same narration text;
- accompanied by a JPEG poster taken from a frame that shows the subject.

The source of each film (its page, script and narration) lives in that repository under `projects/osprey/videos/lesson-<slug>/`. Only the published files are copied into `apps/workbench/lessons/media/<slug>/`. Images are stills of the same films or factory slides, exported at their true size as JPEG, at most 400 KB each. Every film's narration says only what its lesson says, and the Navier–Stokes film keeps to [LESSONS-CURRICULUM]'s account of the published solution and the scope of Osprey's exercises.

## Store and messages [LESSONS-STORE]

The lesson state joins that of [WORKBENCH-STORE]:

| Slice | Held | Contents |
| --- | --- | --- |
| `lessons:catalogue`, `lessons:texts` | host | bounded file-read results, outside the serialized application store |
| `lessons:progress` | host | saved completed challenge identifiers |
| `Shelf` | session | catalogue summaries and faults, the open lesson's slug and source, the focused challenge and loading or save errors |
| `Tutor` | session | per-challenge attempt state including its answer cursor and authored focus epoch, and the completed set ([LESSONS-PROGRESS]) |
| `Achieved` | derived signal | completed challenge keys, so typing an answer does not invalidate the catalogue |

The action union carries `ToLearn`, `ToWork`, `Open slug`, `Answer id`, `FormulaPlace id`, `Key id text`, `Pick id`, `Submit id`, `Hint id`, `Reveal id` and `Carry id` (open in the workbench). The framework delivers typed text, choice and completion messages to `Update::apply`. Judging is a pure function of the challenge and the learner’s input; file and progress operations are bounded host requests.

## Verification [LESSONS-VERIFICATION]

The cases live in `apps/workbench/tests/`, each a fixture through the real `start`/`dispatch` where it concerns the application:

- the reader accepts every block and challenge kind and refuses each broken form: an unknown `kind`, a missing field, a bad slug, an unsafe media path, a formula that does not read, a `math` tree with an unknown node, `format` 2;
- `Maths::decode (Maths::encode t) = t` over a corpus of trees, in `packages/native/tests/`;
- prove: a correct answer is `Correct`; the swapped `and_swap` answer is `Incorrect` at the fourth step with the kernel's message; an unknown sentence is `Unfinished`;
- state: an exact answer, an answer equal by definition (`¬p` for `p → ⊥`), a different but equivalent answer (`Incorrect`), a stray atom and an unreadable formula;
- formula keypad: issued key events through real Update/App routing; start/middle/end and clamped caret insertion, multi-byte arrow offsets, independent attempts, marking invalidation without submission, saved-caret and legacy restoration, retained-editor focus high-water and lesson-qualified identity reset; host acceptance additionally covers the discoverable Implies key, keyboard activation, focus return and narrow layout. A copied private test router is not application integration evidence;
- judge: both choices against a checked proof and a rejected one;
- attempts, hints and the solution reveal follow [LESSONS-FEEDBACK]; editing after `Correct` clears it;
- a keystroke in one challenge's answer re-sends only that `challenge:<lesson>/<id>` region;
- progress round-trips through `StoreWrite` and `StoreRead`, and a missing progress file is no progress;
- every shipped lesson passes [LESSONS-INTEGRITY];
- the `Image` and `Video` encodings, in `packages/native/tests/`.

Each host's smoke test opens `what-is-a-proof`, plays nothing, answers its first challenge and sees the “Right” badge.
