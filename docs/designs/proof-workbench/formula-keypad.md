# Formula keypad — design handoff

**Static design reference.** Open [`formula-keypad.html`](formula-keypad.html) directly from disk. It is a review board, not the application: its buttons do not insert text, submit answers or check proofs. The runtime contract is below.

## The problem

The state challenge [LESSONS-STATE] asks a learner to write a formula in **Your formula**. The field accepts Unicode and the sentence words (`implies`, `and`, `or`, `not`, `iff`), but neither is visible at the point of entry: the empty field offers no symbol affordance. Someone who knows the mathematics can still be blocked by not knowing how to type `→`. The alternative on the board is to draw the symbol, which is a different task.

The keypad removes that prerequisite. The implication arrow is a wide, accented, visibly named key immediately below the field. Nothing else must be learned first.

## What the review board shows

| View | State shown | What it proves |
| --- | --- | --- |
| Paper | The p-only `write-identity` challenge, completed as `p → p`, field focused | The keypad is visible below the field and the arrow is findable in light appearance |
| Slate | The same challenge and field, with the same geometry | The keypad uses the existing Paper & Slate tokens rather than inventing a dark surface |
| Narrow · phone width · p and q | The real `state-swapped-goal` challenge, completed as `q ∧ p`, with both declared variables enabled | The p-and-q variable rule and the narrow wrap, with no horizontal page overflow and no reduced hit areas |

The Paper and Slate views use a real challenge from [`what-is-a-proof.json`](../../../apps/workbench/lessons/what-is-a-proof.json): “Write the claim that p implies itself.” Its expected formula is `p → p` and its only declared variable is `p`. The narrow view uses the real `state-swapped-goal` challenge from [`and-swap.json`](../../../apps/workbench/lessons/and-swap.json): “Write the conclusion with q on the left and p on the right,” expected `q ∧ p`, with both `p` and `q` declared. Both formulas are valid under [`formula.ospml`](../../../apps/workbench/src/formula.ospml), so the board does not show an invented or false statement.

No proof badge appears in the design. “Right”, “Not yet” and “Unfinished” are the kernel-facing feedback of [LESSONS-FEEDBACK] and are not supplied by the keypad.

## Placement and hierarchy

The keypad sits **between the formula field and the existing feedback/actions**, inside the challenge card. It is not a separate pane, popover or drawer, and it never covers the formula.

| Element | Measurement |
| --- | --- |
| Field-to-keypad gap | 14 px |
| Keypad padding | 12 px 14 px (10 px on the phone frame) |
| Key size | 46 × 52 px minimum; 44 × 50 px under 520 px |
| Implies key | 90 px wide (80 px on the phone frame); still at least 44 px high |
| Key gap | 6 px; 5 px on the phone frame |
| Group gap | 18 px horizontally, 12 px vertically |
| Corners | 5 px on keys; 6 px on the keypad and challenge card |
| Focus | 2 px `accent` outline, 3 px offset |

The keypad is a quiet `inset` surface inside the `surface` challenge card. It has one short heading, “Insert a symbol”, and one helper, “Click, or Tab then Enter”. Group labels are sentence case and supplementary; they are not required to understand a key. The optional line below it names the sentence words the grammar already accepts, not backslash codes: the notation expansion table is not currently wired into this field, so this handoff does not promise `\to` here.

The action row keeps Submit, Hint and Open in the workbench. Show solution is omitted because `Tutors::revealableAttempt` is false for state challenges; a permanently disabled control would imply an unavailable path.

## Symbols and wording

Only symbols accepted by the current grammar are shown. [`formula.ospml`](../../../apps/workbench/src/formula.ospml) accepts `∧ ∨ → ↔ ¬ ∀ ⊤ ⊥ ( ) ,` and names; the notation table supplies the same glyphs for `→`, `∧`, `∨`, `¬`, `↔` and the abbreviations.

| Key | Visible name | Tooltip / accessible name | Source |
| --- | --- | --- | --- |
| `p` | — (Variables group) | Proposition p | state challenge `variables` |
| `q` | — (Variables group) | Proposition q — not declared in this challenge | state challenge `variables` |
| `→` | Implies | Implies — if A then B | `Lexer.marks`; `Notation` `to` |
| `¬` | Not | Not — the negation of a proposition | `Lexer.marks`; `Notation` `not` |
| `∧` | And | And — both propositions hold | `Lexer.marks`; `Notation` `and` |
| `∨` | Or | Or — at least one proposition holds | `Lexer.marks`; `Notation` `or` |
| `↔` | Both ways | If and only if — each direction implies the other | `Lexer.marks`; `Notation` `iff` |
| `(` | — (Grouping group) | Open bracket — group an expression | `Lexer.marks` |
| `)` | — (Grouping group) | Close bracket — close a group | `Lexer.marks` |

The seven current state challenges use only variables, `¬`, `∧`, `∨`, `→`, `↔` and parentheses. The keypad is scoped to that fragment: `⊤`, `⊥`, `∀` and `,` are accepted by the grammar but do not occur in any current state answer, so they are not placed in the primary row. A quantifier key without a binder hint or a comma is a trap; if a state challenge later needs one, add a separate Quantifier group containing `∀`, `,` and the sentence-level guidance, rather than adding an unexplained glyph to Connectives.

The reviewed shared [`FormulaKeys`](../../../apps/workbench/src/learnview.ospml) limits this primary pad to the current lesson fragment and uses short paired rows. Key labels combine glyph and plain-language name (`→ implies`); the notation table remains unchanged. The backslash abbreviation table in [`notation.ospml`](../../../apps/workbench/src/notation.ospml) is the glyph authority for the key set, but its `expand` function is not applied to the lesson answer today; the optional help therefore mentions sentence words only.

### Why “Both ways”

“Both ways” is the shortest plain-language label for `↔` that does not require “iff” or “biconditional”. The full name and direction are in the tooltip and accessible name. “Exactly when” was longer and made the narrow connectives row wrap sooner.

## Variables and the disabled state

Variable keys come from the challenge’s declared `variables`. In the Paper and Slate views the challenge declares only `p`, so `p` is enabled and `q` is shown disabled. The narrow view declares both, so both keys enable. A longer declared name such as `smooth` or `blowup` in the Navier–Stokes state challenge grows its key past 46 px and wraps the Variables group rather than truncating. The keypad remains the same shape across lessons, while the enabled set follows the data:

- A p-only challenge disables `q` with the tooltip “Proposition q — not declared in this challenge” and the always-visible note “Only variables this challenge declares are enabled. q appears in lessons that declare p and q.”
- The narrow p-and-q `state-swapped-goal` challenge enables both keys and completes the formula as `q ∧ p`.
- In the p-only view, a symbol click cannot cause the “q is not one of p” error. Typing `q` manually still reaches the existing refusal from `Judge::written`, which must remain; in the p-and-q view `q` is valid.

Use the host’s disabled control semantics and keep the reason in visible text plus the group’s accessible description. Do not rely on reduced opacity alone. If the host cannot expose a reason on a disabled button, use `aria-disabled` on a focusable key, suppress activation and keep the reason in the description.

The current application renders only declared variables; it has no disabled `q`. Showing the stable `q` key is a design proposal for predictability. If the runtime keeps only declared variables, the rest of this handoff is unchanged, but the disabled-key acceptance case does not apply.

## Insertion, caret and focus

The field is a single-line editor with a caret, not a typeset formula tree. Insertion follows the existing inline editor model and the notation rule in [WORKBENCH-DESIGN-SURFACES]:

1. The user focuses **Your formula**. The caret is wherever they left it.
2. Activating a key inserts that key’s text at the caret. If the hosting editor exposes a selection, the key replaces the selection; otherwise the text is inserted between existing characters.
3. The caret moves immediately after the inserted text. The surrounding text is untouched.
4. Focus returns to the formula field, ready for typing. The keypad is not a focus trap. `Escape` from a focused key also returns focus to the field without inserting.
5. Insertion is one edit: one undo step, one change event, one cleared mark. A previous “Right” or “Not yet” mark must not survive text that has changed.
6. Announce the inserted symbol politely to assistive technology (for example “Implies inserted”). Do not move focus to the keypad after a pointer insertion.

The keypad inserts the glyph exactly. It does **not** add invisible characters or auto-format spaces. `p→p` and `p → p` read as the same formula under `Formulas::read`; the board’s `p → p` shows ordinary typed spaces, not a claim that the field typesets automatically. If the field later uses a typeset preview, spacing comes from the `Math` tree, not from inserted text.

Keyboard order is the formula field, then Variables, Connectives, Grouping, then Submit and Hint. Every key is a real button with a visible focus ring on `:focus-visible`; Enter and Space activate it. Group labels and the keypad heading are announced with the group. The arrow keeps its position at the start of Connectives so its location is stable across lessons.

The current [`Tutors::insert`](../../../apps/workbench/src/tutor.ospml) already inserts at the stored cursor, advances it by the inserted length and clears the mark. Returning keyboard focus to the field is a host/runtime requirement to verify; the action that inserts text currently focuses the challenge, not necessarily the editor.

## Feedback, disabled and error states

The keypad never decides a verdict. It stays present and unchanged while the existing lesson feedback does its work. Submit is disabled only while the trimmed answer is empty, matching `Tutors::readyAttempt`.

| State | Presentation | Must not imply |
| --- | --- | --- |
| Empty | Submit disabled; no error; keypad usable | That the empty formula is wrong |
| Composing | No badge; ordinary caret and field focus | That a previous check applies to edited text |
| Unreadable formula | Existing `Unfinished` caution badge and the parser’s “expected … at column …” message; text is preserved | That the formula is mathematically false |
| Different proposition | Existing `Not yet` danger badge and the kernel’s reason; keypad unchanged | That the keypad chose a different answer |
| Correct | Existing `Right` good badge, only after the kernel accepts; keypad remains available for revision | That the keypad itself checked anything |
| Disabled variable | Opacity plus tooltip and visible reason; activation suppressed | That the variable is missing from the lesson |
| Narrow viewport | Groups wrap; field, keypad and actions remain in reading order | That horizontal scrolling is required |

Do not turn an unreadable formula into a red proof rejection. A parse fault is “Unfinished”, not “Not yet”; a well-formed but different proposition is the reverse. Do not show a success checkmark while the answer is still being composed.

## Appearance and type

The lesson frames use the canonical tokens from [WORKBENCH-DESIGN-COLOUR] and [WORKBENCH-DESIGN-TYPE]. No new colours are added inside those frames, and no semantic success colour is used for key selection; the review-board chrome outside the frames uses a neutral studio background only.

| Role | Paper | Slate | Use here |
| --- | --- | --- | --- |
| `canvas` | `#FAFAF7` | `#202723` | Lesson page |
| `inset` | `#F0F2ED` | `#191F1C` | Keypad surface |
| `surface` | `#FFFFFF` | `#29322D` | Challenge card and ordinary keys |
| `primary` | `#242923` | `#ECEDE5` | Formula, lesson title, key glyphs |
| `secondary` | `#56615B` | `#B7C0B7` | Helper, group labels, key names |
| `accent` | `#36635C` | `#A6C5BB` | Focus ring, Implies key border and name |
| `outline` | `#727C74` | `#7D8C80` | Key boundaries |
| `divider` | `#D7DDD5` | `#465348` | Field rule, card boundary |
| `selection` | `#E4ECE5` | `#34473E` | Implies key background |

Type: STIX Two Math for glyphs, STIX Two Text for the lesson title, summary, prompt and field, Geist for controls and labels. The lesson title is 34 px (27 px phone), the prompt 21 px (19 px phone), the field 28 px (25 px phone), key glyphs 20 px, key names 10.5 px, group labels 11 px, helper/notes 12 px. The keypad uses the 8-point rhythm with 4-point subdivisions; the 46 × 52 px key and 90 px arrow are the smallest geometry that keeps a comfortable target and a readable name.

The arrow key is deliberately wider and uses `selection` plus an `accent` boundary and name. Its position, size, name and tooltip all identify it, so the accent colour is not the only evidence.

## Narrow viewport

The phone frame targets 390 px; at a 390 px page viewport the review board’s 16 px margins make it 358 px. There is no horizontal page overflow at that width, and the keypad is one column of groups beneath the field. The narrow view uses the p-and-q challenge, so both variable keys are enabled. Variables, Connectives and Grouping wrap independently; the Implies key remains first in Connectives and full-size. The challenge padding drops to 20 px, the field to 25 px, and the action row keeps Submit, Hint and Open in the workbench on one line with the status below. Nothing is hidden and no control shrinks below 44 px. The phone view in the board is a static representation of this geometry.

## Runtime boundary and next work

The HTML is a static review board, not an application implementation. It contains no insertion, checking or theme logic; the two appearances are separate markup using shared classes, and no host or app source was changed for this handoff.

The production implementation belongs in the shared Osprey application and `@osprey/native`, not in a host-specific transplant of this HTML:

- Reuse `Editors::inline` and the caret/cursor model from [`learnview.ospml`](../../../apps/workbench/src/learnview.ospml) and [`tutor.ospml`](../../../apps/workbench/src/tutor.ospml).
- Keep the keypad immediately after the field, before `Feedback::marks` and `Feedback::controls`. The shared `posed` function now authors that order.
- Build the key set from one ordered table containing symbol, visible name, tooltip and accepted-fragment membership. Do not duplicate symbols in the host or in lesson JSON. The notation table remains the spelling authority.
- Keep the disabled-variable decision explicit and test it with a p-only and a p-and-q challenge.
- Add acceptance for pointer insertion, keyboard tab/Enter/Space, caret position, focus return, mark clearing, one-step undo, empty Submit, unreadable feedback, variable refusal, 390 px overflow, focus contrast and reduced motion.
- Do not attach a proof badge, a “checked” style or a verdict to a keypad key. Only the kernel-facing feedback of [LESSONS-FEEDBACK] may do that.

This handoff does not claim the keypad is implemented, that the current runtime matches the pictures, or that a design review substitutes for browser/native acceptance. The current tests and the production suite remain the authority.

## Verification of this artifact

The source artifacts and their local asset paths have been checked. The design pi reported Chromium checks but retained no screenshots or console receipt, so those claims are not accepted as visual evidence. The parent browser security policy refused local-file preview; no alternate browser route will be used to bypass that refusal. Actual shared-app screenshots and installed native/VSIX interaction checks remain required.

## Open decisions for implementation

- The reviewed primary pad omits `forall`: no current state challenge needs it, and a quantifier needs a comma and binder guidance. Quantifier entry remains ordinary text.
- Show undeclared variables as disabled keys (this design) or omit them (current runtime). The disabled version is more stable; the omitted version is quieter.
- Label `↔` as “Both ways” (this design) or “Iff”/“Exactly when”. The tooltip carries the formal name in every case.
- Confirm whether the host can restore focus to the inline editor after a key action. That is the only behaviour in this handoff that the current insertion path does not obviously guarantee.
