# Proof Workbench design QA

**Final result: passed** for the standalone design artifact and demonstrated workflows. This is not production workbench acceptance or a proof-kernel qualification.

## Evidence

- Original visual direction: `paper-reference.png` (1487 × 1058 image pixels).
- Revised implementation: `screens/paper-proof.png` (1488 × 1058 CSS/image pixels, device scale 1), captured at the equivalent point before the fifth step.
- Original and implementation were opened together in one comparison call. The one-pixel width difference is immaterial; no density rescaling was used. This comparison checks the visual language, not pixel identity: later user instructions explicitly changed the default to a writable board, required free-form solutions and requested additional workflows.
- Other inspected captures: `board-first.png`, `blank-board.png`, `working-session.png`, `mismatch.png`, `optional-source.png`, `reading.png`, `slate-board.png`, `transcription.png`, `tablet-board.png` and `phone-board.png` in `screens/`.
- Desktop viewport: 1488 × 1058. Tablet: 834 × 1194. Phone: 390 × 844. Full-page captures preserve scrolling content where appropriate.
- Video: `video/walkthrough.mp4`, H.264, 1440 × 1080, 53.96 seconds, silent. Representative mismatch and source frames were extracted and visually inspected; temporary frames were removed afterward. Nine chapter captions are visible in the video; a VTT file is included.
- Browser verification used an isolated local Chromium because the in-app browser host was unavailable. Both the gallery and demo were tested using direct `file:` navigation after removing the development scaffold.

## Comparison history and fixes

1. Initial paper mock established the mathematical serif, independent paper/ink/mineral palette, goal margin and slate scratch surface. Subsequent user instructions made the board the main surface and required an entire editable solution, so the mock’s lower board layout is now the alternate Paper view.
2. Two independent reviewers found unreadably small support text, a large salmon diagnostic, a tablet layout that squeezed the board, a source popup covering goals, and an intrusive save/restore toast. Their findings came from actual captures and source inspection.
3. Supporting text was enlarged; the diagnostic became a compact local annotation with readable formulas. Tablet now uses a full-width board with the document below. Source docks as a companion pane and wraps long lines. Save status initially moved beside the title; the fresh review below replaces that hidden-on-mobile location with a persistent-width status strip.
4. The workflow review found that reset discarded board strokes/notes and that the tour did not restore the initial workspace. A failing regression reproduced reset data loss. Reset now preserves working; the tour snapshots and restores its starting state. The background workspace is inert during the tour so a user cannot make edits that playback would discard.
5. The default changed from a fixed lesson to a blank board plus editable title and free-form solution. The example is separate. Unsupported mathematics remains visible and unchecked.
6. The visual re-review confirmed that the mismatch, typography, source docking and tablet-width issues were materially fixed, leaving compact-board framing and source clipping. A Fit control now frames the fixed board world; Paper keeps the same board behind an explicit disclosure. Source wraps within its pane and retains the selected line and gutter.
7. Direct file opening previously produced an empty page. A browser regression first asserted one rendered workbench and received zero. The final HTML embeds code, fonts and artwork; the unchanged regression now passes without a server. The gallery also loads every screenshot and the video metadata directly from disk.

## Fresh review · 29 October 2026

Three independent agents reviewed current browser captures and interactions: visual composition, workflow/keyboard behavior and mathematical credibility. Each rechecked its findings after the fixes. No remaining P1/P2 visual defect or severe workflow/mathematical defect was found in the reviewed states. This is bounded review evidence, not a general usability or accessibility certification.

| Step | Finding and correction | Current evidence / health |
| --- | --- | --- |
| 1. Start and write | Repeated promotional/verification prose crowded the writing pane. Replaced it with a labelled writing area and short status; sample transcription appears only in the example. | [Blank board](screens/blank-board.png), [working](screens/working-session.png): clear |
| 2. Inspect source on phone | Source opened 1,239 pixels below the screen; the header’s return label wrapped across three lines. Source now appears before the document and receives focus; return has its own contextual row. | [Phone source](screens/phone-source.png): visible immediately |
| 3. Save and reopen | Import errors disappeared with the narrow header, and importing reset pan/zoom. A visible announced status strip keeps errors until dismissed; bounded view transforms survive reopening. | Six state tests and mobile import regression: pass |
| 4. Write notation | Symbol insertion appended to the end, ignoring the caret. It now inserts/replaces the selection and restores editing focus. | Real keyboard/caret regression: pass |
| 5. Clear or replay | Clear and tour did not receive keyboard focus. Native dialogs now contain focus, support Escape and restore the prior working. | Focus, Tab cycle, Escape and workspace restoration regressions: pass |
| 6. Interpret feedback | A valid but unsupported paraphrase incorrectly got “The goal and this step differ.” It now gets an amber scope explanation with no claimed proposition or invalid-input assertion. Known mismatch still compares q with p. | [Unsupported input](screens/unsupported-input.png), [mismatch](screens/mismatch.png): distinguished correctly |
| 7. Read the argument | Isolated p/q were plain prose in step 3; they now use the math renderer. Removed green checkmarks from the unchecked completion state. | [Reading](screens/reading.png): consistent notation and neutral status |
| 8. Work on tablet | The sample formula overlapped the bottom status. The drawing viewport now excludes the control strip; image edges blend into the board when fitted. | [Tablet](screens/tablet-board.png): no overlap, crop or hard photo edge |

Seven new browser regressions first exposed the corresponding existing failures; the original direct-file check remained passing. After correction, all eight pass. The broader workflow capture also passes with no page errors, and desktop/tablet/phone captures were refreshed. The 54-second browser video was re-recorded from the revised artifact.

## Fidelity surfaces

- **Typography:** real STIX Two Text and STIX Two Math are embedded; the browser font-load check passes. Formula layout uses native MathML. Control text uses Geist. Main proof text remains serif, source remains monospaced. The title is deliberately smaller in the board side pane. Arbitrary free-form working is not parsed or typeset as a checked formula by this prototype.
- **Spacing:** continuous proof page, quiet goal margin, board as the larger desktop region, full-width tablet board. Added navigation supports the requested workflows. Paper’s board disclosure is intentional, not a missing region. Phone and tablet checks find no horizontal page overflow.
- **Colour:** application tokens follow the canonical spec’s Paper & Slate palette. Mismatch has local red emphasis, not a full-width tinted card. Text, symbols and position supplement colour.
- **Images:** the supplied chalk study remains an explicitly labelled sample. User strokes are actual canvas input in a fixed coordinate system. The generated reference is not flattened into a fake interactive application. The production chalk shader and editable sample strokes remain separate implementation work.
- **Copy and content:** no programming knowledge is required. Optional source explains its fixture scope. Unchecked status remains visible after all draft goals close. Recognition explicitly describes a sample preview. Unknown fixture sentences are retained with a visible limitation.
- **Focused review:** mismatch formulas, source wrapping and active-line treatment, tablet board framing, current-step and goal alignment were inspected in the full-resolution captures; these were the specific surfaces raised by the reviewers. No conclusion was drawn from thumbnails alone.

## Functional verification

Passed browser interactions: drawing; undo enablement; board clear confirmation; save/open retaining strokes and notes; wrong-step feedback; repair and draft completion; optional source visibility; font loading; tablet and phone overflow; reduced-motion styling; tour snapshot restoration; inert underlying workspace during the tour. No browser page errors were emitted by that run.

Six state regressions pass: preserve working on import without restoring a checked verdict; reject unknown/malformed workspace data; preserve the argument on a mismatch; complete the known fixture without inventing a verdict; preserve strokes and notes when restarting the proof; preserve valid saved board framing and reject malformed transforms.

Verification sources are retained in `prototype/checks/`. They use Osprey’s existing website Playwright installation, not a nested package project. The demo itself has no runtime dependency on Node or Playwright.

## Limits and follow-up

- The prototype is a finite design fixture, not the production Osprey application. No live checker, handwriting service, imported theorem library or automatic persistence is claimed.
- Real user strokes have simple immediate rendering; the application’s textured chalk shader, platform pen behaviour and assistive-technology contracts require host implementation and acceptance tests.
- The mathematical audience has not participated in a usability study. Independent agent critiques help find defects but do not establish that mathematicians will love the product.
- The video and screenshot set were refreshed after the latest review fixes.
- Repository-wide CI and Deslop were not run. Deslop tools/CLI were unavailable in this session. Verification was scoped to design artifacts; production compiler and application source were not modified here.

## Production integration checkpoint · 29 October 2026

The subsequent application work uses `@osprey/native`; the React study remains a visual reference. Osprey owns proof logic, board transitions and persistence. The shared browser renderer now supports typed MathML, marked editors, separate Unicode caret bindings, application palette/motion tokens and a canvas board with textured chalk. Board runtime data uses a base framing/notes slice plus separately indexed stroke slices; the complete workspace save retains the whole board. Opening a generated-source pane must remain an explicit choice.

Evidence at this checkpoint: 44 native-framework cases passed under ARC. Forty focused host checks passed, including real webview pointer drawing through a background render, one event per completed stroke, retained text-control provenance and palette retention after dispatch. Earlier compiled SDK browser checks verified actual Osprey-to-WASM MathML and Unicode editor/caret behavior. The shared board model passed its initial GC/ARC cases before the ongoing application modules were added. Verification scripts and logs are under `vscode-extension/test/suite/` and `target/workbench-*.log`; these are component checks, not application acceptance.

Claude owns the application/store/checker/lesson integration; TMC carries the interface and design guidance. Full application entry points, host media/font packaging, complete screens, native-host execution and the phase exit gates remain unfinished. The latest application check is blocked by a `Checks` declaration collision in the actively edited store/tutor modules. No complete application sign-off is claimed by this checkpoint.
