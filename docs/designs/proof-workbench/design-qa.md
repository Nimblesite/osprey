# Proof Workbench design QA

**Final result: blocked** for production workbench acceptance. The standalone design artifact passed its historical review; that does not qualify the production application or proof kernel. The current production review below is authoritative for delivery.

## Scope correction · 30 October 2026

The user withdrew the Workbench freehand board and handwriting workflow. Board-first layouts, drawing, lasso, recognition previews and board-focus interactions described below are historical design evidence, not current delivery requirements or recognition capability claims. The supported workflow uses the mathematical document, typed formulas and proof steps, with optional source. Existing recorded films may show the older controls; their presence does not demonstrate mathematical handwriting recognition.

Retiring these controls must preserve existing saved strokes, notes and framing and the generic native SDK drawing/recognition APIs. The application owner has sealed a no-ink source checkpoint; fresh native and installed-VSIX interactions and visual acceptance remain separate requirements. Keep typography, exact checked statements, assumptions, caret/focus, overflow and error-state findings actionable on the supported surfaces. Do not treat historical board acceptance as evidence that the revised application is accepted.

## Historical design evidence

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

At this earlier checkpoint, Claude owned the application/store/checker/lesson integration and TMC carried the interface and design guidance. Application entry points, host media/font packaging, complete screens and native execution were unfinished, and the then-current application check was blocked by a `Checks` declaration collision in the store/tutor modules. That compiler blocker has since been resolved: the current 108-case GC/ARC/WASM and coverage receipts appear in the production review below. This paragraph retains historical component evidence and is not a current blocker report or a complete application sign-off.


## Production acceptance review · 30 October 2026

Final result: blocked

Reviewed 2026-10-30 Melbourne time. Visual truth is `docs/designs/proof-workbench/screens/`. The implementation uses the existing shared Osprey application and native framework. This is an iteration report, not a pixel-perfect signoff.

## Evidence and normalization

The first root review used browser-rendered captures from `target/workbench-vsix/` and combined each reference and implementation into a single comparison image under `target/workbench-vsix/design-qa/`. Reference is on the left. These are content captures without browser chrome at density 1; no resampling was applied. The initial review aligned image pixels but did not establish the source CSS viewport, so its geometry deltas are provisional until the corrected captures below.

| State | Reference pixels | Initial implementation pixels | Combined comparison | Focused comparison |
| --- | --- | --- | --- | --- |
| Optional source | 1488 × 1058 | 1488 × 1058 | `target/workbench-vsix/design-qa/optional-source-comparison.png` | `target/workbench-vsix/design-qa/optional-source-focus.png` |
| Slate board, completed example | 1488 × 1102 | 1488 × 1102 | `target/workbench-vsix/design-qa/slate-board-comparison.png` | `target/workbench-vsix/design-qa/slate-board-focus.png` |
| Transcription preview | 1488 × 1098 | 1488 × 1102 | `target/workbench-vsix/design-qa/transcription-comparison.png` | `target/workbench-vsix/design-qa/transcription-focus.png` |

Viewport correction: `docs/designs/proof-workbench/prototype/checks/capture.mjs:9` sets the desktop CSS viewport to **1488 × 1058**, and line 13 uses `fullPage: true`. Thus transcription's 1098-pixel image and slate's 1102-pixel image are scrollable content heights, not CSS viewport heights. Root's earlier instruction to use those image heights as the CSS viewport was incorrect and has been superseded. The host owner is measuring the reference DOM and will recapture at the actual source viewport with full-page content. The approximately 184-pixel visible transcription board deficit remains an observation of the initial images, not a normalized acceptance measurement. The remaining eleven reference states still need corresponding combined comparisons.

Native renderer evidence is separately combined in `target/workbench-vsix/design-qa/optional-source-native-comparison.png` and `slate-board-native-comparison.png`, using `target/workbench-pixel/optional-source-native-current.png` (1488 × 1058) and `slate-board-native-current.png` (1488 × 1102). These match the reference image sizes, which alone does not establish matching viewport/scroll behavior. Actual native-window screenshots are approximately 2898 × 1868 physical pixels and represent a different viewport at density 2; they are interaction evidence, not an exact reference match.

## Findings

- **[P1] Transcription board and footer collapse upward.** The reference board reaches approximately y=990; the implementation reaches approximately y=806. Chalk content is consequently smaller and the page footer leaves a large blank region below it. Evidence: transcription full-view comparison. Restore the shared board's viewport-dependent height and recapture at 1488 × 1098. UI and VSIX owners are coordinating the fix.
- **[P2] Transcription primary action loses its fill.** “Use this sample statement” is a green filled button in the reference and plain text in the implementation. Evidence: transcription focused comparison. Restore the existing shared primary-button styling and verify both hosts.
- **[P2] Toolbar icon families differ.** Board navigation, pan, transcription and theme controls use different symbols from the reference. Evidence: slate focused comparison and transcription full view. Bundle the reference's existing icon family through the shared asset path; preserve native and browser accessibility labels. The VSIX owner owns the generic renderer change.
- **[P2] Document completion rhythm drifts.** In optional-source, the restart action loses its green foreground; its divider and completion footer sit approximately 10–20 pixels lower and the divider is darker than the reference. Evidence: optional-source focused comparison. Correct the shared link style, spacing and divider token. The native/UI owner owns geometry.
- **[P2] Close focus state and alignment differ.** The reference's apparent larger close border includes its focus ring around a 32-pixel button; the implementation capture has no corresponding ring. This does not justify enlarging the button. The browser transcription glyph is also left-aligned within its control. Evidence: both full views and transcription focus. Match the reference's keyboard focus state, retain correct 32-pixel bounds, and center the existing icon.
- **[P2] Native goal boundary differs.** In the initial native slate comparison, the separator between completed proof and “AT THIS STEP” is absent and the goal content begins approximately 20 pixels above the reference. The native optional-source completion divider also spans the full document row. Evidence: both native combined comparisons. Restore the shared separator and padding without changing the result content. Native punctuation and caption-height fixes announced after these captures need new visual evidence.
- **[P1, implementation fixed; visual acceptance open] Imported theorem statements were elaboration dumps.** The earlier bundled arithmetic capture renders `HAdd.hAdd` application details as the statement. Evidence: `target/workbench-vsix/bundled-arithmetic-proof.png`. The shared adapter now presents the verified term through the existing mathematics tree and retains the exact formal statement for inspection; the final 108-case GC, ARC and wasm32 suites prove that flow. Revised host captures still need comparison before this visual finding closes.

## Required fidelity surfaces

- **Fonts and typography:** the reviewed browser states use the reference's serif heading/body treatment and sans controls; mathematical glyphs and heading wrapping are close in the combined views. The imported-statement correction has passing application regressions; its revised host presentation still requires comparison. Native font metrics and all remaining states still require comparison.
- **Spacing and layout:** slate board geometry is close in this capture. Transcription board height, completion spacing and close-control alignment remain actionable above. Phone/tablet and other reference states have not passed root review.
- **Colors and tokens:** paper/slate backgrounds and selected proof-row fills agree visually. The transcription action, restart foreground and divider differ. No claim of exact color equivalence is made before these fixes and final sampling.
- **Image quality and assets:** the actual chalk artwork is reused and appears sharp. Native paper and slate artwork are visible in the exact-size combined comparisons; the earlier opaque paper-compositing failure is absent there. Its browser transcription scale differs because of board geometry. Toolbar icons differ and are being replaced with the reference family.
- **Copy and content:** truthful kernel results intentionally replace prototype claims such as “No checker on this host.” Retain actual theorem names, assumptions and refusal states. The prototype's simulated transcription remains explicitly identified as a sample. These accuracy changes are required; spacing must accommodate them.

## Interaction evidence and remaining checks

The sealed VSIX receipt `target/workbench-vsix/final-delivery-receipt.json` reports 41 focused browser cases, 806 full extension tests and 169 installed-extension tests, with no source drift for that receipt. The actual bundled Nat, OpenAI and large List flows check proof source and expose assumptions. This proves those interactions, not visual fidelity after subsequent fixes. Console errors and final screenshot provenance must be rechecked with the revised captures.

An earlier root CUA inspection of `/Applications/Visual Studio Code.app` found that the existing Osprey profile did not yet expose `Osprey: Open Workbench`. The host owner subsequently installed and reloaded the extension and verified the command in that profile; the final revised source still needs its own profile check. The installed-extension test fixture's receipt does not establish that the user's existing profile has been updated. The VSIX owner has been assigned final installation and an actual proof check in that profile after the next source seal. The palette was closed without changing the user's editor content.

## Comparison history

1. Initial root combined review: the transcription, icon, completion and close-control findings above remain open. Owners received exact evidence and viewport correction through TMC. No post-fix comparison has been accepted yet.
2. Earlier offscreen imported-result bug: the shared compact theorem picker and result-first inspector made the verdict visible in the actual bundled-proof captures. Exact statement readability remains open; this is not full closure of the imported-proof presentation.
3. Imported-mathematics correction: a regression scoped to `proof-library-result` failed with a missing math node and `null` instead of the exact `Nat.add_comm` tree. The shared adapter now retains the verified term, and `ProofTerms` uses the existing mathematics tree while preserving the raw view. Separate global-constant collisions (three failed assertions) and sort-label collisions (two failed assertions) were pinned before their fixes. The actual flow produces the required formula tree; its raw-view oracle compares the exact text field instead of a helper that adds separator spaces. All 108 application tests pass under GC, ARC and wasm32, with unchanged final source seals and 90.8% own-source coverage. Revised host receipts and post-fix screenshots remain pending, so the visual finding is not closed yet.
4. Reference viewport correction: the source capture script establishes 1058 CSS pixels plus full-page output for the desktop states. Image height was previously conflated with viewport height. Revised captures must establish both dimensions before geometry findings can close. The filled-action and icon-family differences remain directly visible regardless of this normalization correction.

## Later production checkpoint — 30 October

The `aa987026…` shared WASM passes fourteen actual browser gallery states and the `run_work` statement, verdict, assumption and complete mathematics bounds check. A subsequent settled-capture run adds the phone source heading/close-control clearance assertion and reports fifteen passing gallery cases; the apparent overlap did not reproduce geometrically. Full-page chunks are stitched for comparison. These interaction and bounds results do not by themselves certify pixel identity.

The shared application checkpoint passes 113 cases on GC, ARC and wasm32 with zero skips and no input drift. See `target/proof/openai/notation-identity-review/memo-full-app-gc-receipt.json`, `memo-full-app-wasm-mounted-receipt.json` and `target/film-review/library-layout-final-full-arc.log`. This predates the next navigation and bounded-mathematics fixes. The native `c7abba4e…` archive is a fresh no-drift checkpoint after rebuilding the stale ARC runtime; its `run_work` capture exposes the complete statement. The initial scrollbar assertion at 821 pixels was a false alarm because this formula fits that pane. The corrected actual-window test at 430 pixels retains every scroll/end-visibility assertion and passes; no renderer workaround was introduced. Actual native and VSIX tests also fail when opening the conjunction example leaves library inspection over the proof.

The shared fixes and their source hashes are sealed in `target/workbench-pixel/navigation-math-source-seal.json`; fresh host reruns are pending. The native picker contrast regression has a separate tested repair. Five-colour still misses the existing three-second theorem-option readiness deadline; panel cancellation now terminates its pending worker. No timeout, proof check or mutation assertion is relaxed. Final pixel review, installed-profile verification and refreshed proof footage remain open.

## Implementation checklist

1. Fix the recorded shared/native/browser geometry and styling regressions with their existing failing interaction or renderer tests.
2. Recapture the same reference state at its exact viewport and density; retain before/after comparison evidence.
3. Review the corrected mathematics presentation in fresh host captures, preserving exact formal source and actual assumptions.
4. Compare every supplied reference state on its applicable host, including phone/tablet and error states; check focus, keyboard controls, overflow and console errors.
5. Refresh relevant host tests and package receipts after the final source changes. Mark passed only after all actionable P0/P1/P2 findings close.

No follow-up polish is being used to defer these acceptance blockers.

## Current visual review — 30 October

The frozen `e61af037…` host snapshot passes 75 host/gallery/session cases, including fourteen stitched screen states; these results precede the latest shared viewport metric. Parent comparison still identified a pale transcription action on hover and excess phone source/article inset. The VSIX owner is repairing these against pinned hover and geometry regressions. The native generic viewport and keyboard suite reports 78 passing cases; actual current application captures and final keyboard acceptance remain required. Five-colour readiness still exceeds the unchanged three-second deadline. No final pixel-perfect acceptance is recorded.
