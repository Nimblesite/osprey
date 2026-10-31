# Proof Workbench — design handoff

**Historical prototype.** The user retired the freehand board and handwriting workflow. The current Workbench uses mathematical documents, typed formulas and proof steps, with optional source. Follow [the current implementation contract](../../specs/applications/0051-ProofWorkbench.md) and [production acceptance status](design-qa.md); the board workflows and acceptance references below record the earlier design study. Fresh native and installed-VSIX acceptance remains required for the revised application.

Explore the historical [visual gallery](index.html) and [interactive demo](prototype/index.html). Both open directly from disk. No server, package installation or separate repository is needed.

This is the Paper & Slate design direction, revised with the user and reviewed with Claude through TMC. It is a design artifact inside Osprey. It is not the production application or a proof checker.

## Historical product direction

A place to do the whole mathematical solution: draw, calculate, write reasoning, organise the argument and read the finished page. The board is the primary surface. Nobody needs to know Osprey. Generated, syntax-highlighted source is available only when explicitly opened. Arbitrary working remains visible and unchecked; supported proof steps may gain precise feedback from the real checker when implemented.

Inspired by Osprey’s precision and restraint; independent colour, typography and composition. The current screens use a typographic wordmark. The earlier symbol studies are exploration only, not an approved implementation asset.

## Historical implementation handoff

[Spec 0051, WORKBENCH-DESIGN](../../specs/applications/0051-ProofWorkbench.md#beautiful-or-not-shipped-workbench-design) defines the exact colour tokens, type scale, mathematical spacing, surfaces, feedback and motion. [Plan 0042](../../plans/0042-proof-workbench.md) defines delivery order. Implement production behaviour in the Osprey application and the existing native host framework, not by transplanting this JavaScript design study.

The approved [optional source](screens/optional-source.png), [slate board](screens/slate-board.png) and [transcription](screens/transcription.png) PNGs are the mandatory visual acceptance contract for the shared Osprey implementation. Preserve these references. Compare actual native and VSIX captures at the same client viewport, excluding operating-system chrome; extend `@osprey/native` when a host cannot express the authored geometry. Runtime verdict wording must describe the real checker rather than repeat the study’s unchecked fixture labels. Native pixel-perfect acceptance is still outstanding.

The spec’s board, store and regions were updated by Claude for the board-first workflow. The design section was updated here for Paper & Slate. Runtime tests remain the authority for shipped behaviour.

## Screens

These are browser-rendered captures of the interactive study, not generated claims of a working compiler feature.

| Screen | What implementers should study |
| --- | --- |
| [Blank working board](screens/blank-board.png) | Initial authoring surface; writable board and editable solution |
| [A working session](screens/working-session.png) | Unrestricted written calculations, always unchecked in this prototype |
| [Guided board](screens/board-first.png) | A sample sketch with its related proof beside it |
| [Paper proof](screens/paper-proof.png) | Serif mathematical argument, current step and goal margin |
| [Mismatch](screens/mismatch.png) | Local diagnostic, two readable formulas, repair action |
| [Unsupported input](screens/unsupported-input.png) | Outside the finite example, without inventing a mathematical rejection |
| [Optional source](screens/optional-source.png) | Docked companion pane, semantic highlighting, selected-line link |
| [Reading](screens/reading.png) | Continuous mathematical page, verification status retained |
| [Slate](screens/slate-board.png) | Low-glare board appearance with readable semantic colours |
| [Transcription preview](screens/transcription.png) | Explicit review before insertion; recognition is simulated |
| [Tablet](screens/tablet-board.png) | Full-width board with proof below, not a crushed desktop sidebar |
| [Phone](screens/phone-board.png) | Narrow fallback; touch controls and no horizontal page overflow |
| [Phone source](screens/phone-source.png) | Explicitly opened source is immediately visible above the document |

[Original paper composition reference](paper-reference.png) is an ImageGen study. It predates the board-first revision and is not the current default layout.

## Workflows

### Start and keep the whole solution

Open the demo to a blank board. Give the solution a title and write any reasoning or calculation in the solution field. Draw with a mouse, pen or touch pointer. Pan and zoom retain a fixed coordinate space. Working notes are available from the board toolbar. These inputs are deliberately unrestricted and unverified.

Save downloads a workspace JSON containing the written solution, proof fixture state, strokes and notes. Open restores these values, including the board’s pan and zoom, and keeps the status unchecked. Save and import feedback appears in a dedicated status strip at every screen width, never over the board. Import errors remain visible until dismissed. The prototype does not automatically persist across browser reloads; save before closing. A production save format and interruption-safe persistence are separate implementation work.

### Work on the board

The board takes the larger portion of the desktop workspace. Focus hides the proof region without creating another canvas. Undo removes a user stroke. Clear asks for a second action and preserves the written solution and notes. On tablet, the board spans the width and the accompanying document follows below. Board coordinates do not stretch when the viewport changes. A separate bottom strip keeps the status and zoom controls clear of the mathematics.

The guided tour’s chalk diagram is an image-backed design example; its individual marks are not editable strokes. User drawing is real canvas input. Production chalk rendering must operate on the actual stroke model from spec 0051.

### Turn a sketch into a statement

The sample transcription action appears only in the guided example and opens a clearly labelled preview. The author reviews it and chooses whether to insert it; the board remains intact. This prototype has no handwriting-recognition service and never claims to recognise arbitrary strokes. Production uses the `Recognise` capability and shows its unsupported state explicitly.

### Write and inspect a proof

The guided `and_swap` example accepts the documented five sentences and exposes the goal after each. Select a previous step to inspect that point in the argument. Enter `From hp.` when the goal is `q` to see the mismatch. The diagnostic compares the two propositions and offers the example step as an explicit action. Unsupported input is retained with a neutral amber limitation, never a claimed mismatch or a claimed result. A valid paraphrase outside the fixture is not presented as false.

Closing the last draft goal does not fill a checked badge. No kernel is connected to this design study. The production goal engine, checker tickets, rejection handling and disagreement tests belong to the application.

### Choose notation or an example

Notation replaces the selection or inserts at the caret in the current working field, then restores editing focus. The examples panel contains the one supported design fixture and makes that limitation visible. Restarting the example preserves board strokes and notes. Return to working leaves the guided proof and restores the authoring surface; the written solution remains intact.

### Inspect generated source, if desired

Source is closed initially. In the guided example it docks beside the proof, uses semantic keyword/type/name/operator/comment/hole colours and highlights the selected step’s generated line. It is read-only and can be exported. On smaller screens it appears before the document and receives focus, so opening it has an immediate visible result. Free-form working cannot be compiled by the prototype; opening source explains this rather than inventing a program. Production must use the real emitter and line map.

### Read and export

Read shows the same mathematical argument as a continuous page. Print / PDF uses the browser print flow, retaining the verification label. Save preserves editable working. Source export is separate and optional. A future production paper export uses the real `Math` tree and MathML contract.

### Replay the working session

Play walkthrough runs nine six-second chapters: board, transcription preview, proof writing, mismatch, goal transition, draft completion, optional source, reading and slate appearance. Pause, previous, next and close are live controls. Closing the tour restores the workspace that was open before the tour. It never converts the author’s working into the fixture.

[Walkthrough video](video/walkthrough.mp4) records the actual demo. Its captions are part of the design walkthrough, not production UI. It is silent and contains no implied checker execution.

## Motion and access

The exact motion values are in [WORKBENCH-DESIGN-MOTION](../../specs/applications/0051-ProofWorkbench.md#motion-workbench-design-motion): controls 120–150 ms; panes 220–260 ms; goal changes 280–350 ms; surface changes 300–380 ms. Motion explains the changed region; the rest stays stable. Reduced motion removes translation and decorative animation. Pointer drawing remains immediate.

All mathematical prose uses STIX Two Text, formulas use STIX Two Math and controls use Geist. [The STIX project](https://www.stixfonts.org/) distributes its fonts under the SIL Open Font License. Font and icon licence texts accompany the demo. Focus, state labels and diagram position supplement colour; status is never colour-only. Contrast measurements are in the spec. Production assistive-technology and handwriting acceptance tests are still required.

## Review decisions

Independent visual and workflow reviewers identified five priority defects in the first capture: the tablet sidebar crushed the board; the chalk example had visible texture seams; the mismatch panel was oversized with tiny text; source covered the goal; and notifications obscured working. They also found that restarting the example cleared strokes and notes and that the tour did not restore the previous workspace.

The revised study uses a full-width tablet board, one fixed board coordinate system, compact diagnostics, readable support text, a docked source panel and a visible status strip at every width. Example reset preserves working, and the tour restores its starting state. The default is editable free-form work, with the constrained example separated into the walkthrough.

A fresh round with three independent reviewers covered visual design, working interactions and mathematical credibility. It corrected hidden phone import errors, off-screen source, cramped navigation, missing dialog focus, caret insertion, lost camera state and an unsupported-input diagnostic that incorrectly claimed a mismatch. Native dialogs now support Escape and contain keyboard focus; draft completion uses neutral text while unchecked.

[Design QA](design-qa.md) records the captures, functional checks and remaining limitations. The standalone-file regression proves that opening `prototype/index.html` directly renders the application; it no longer needs Vite or localhost.

## Artifact provenance

The chalk example and original visual studies were made with the built-in image-generation tool. The interactive screens and video are real browser captures. [Prompts](prompts.md) retain the final image briefs. No source assets are fetched from the network when opening the final demo.

## Files for implementers

`prototype/index.html` is the self-contained playable artifact. `prototype/source/` retains the authored design-reference source; it is not another project or the production implementation. `prototype/checks/` retains the browser/state verification sources, using the repository’s existing website browser tooling. `prototype/LICENSES/` retains embedded third-party notices. There is no nested `AGENTS.md`, package manifest, hosting scaffold or `node_modules` directory.

Run the state regressions from the repository root with `node --test docs/designs/proof-workbench/prototype/checks/workspace.test.mjs` (six passes). From `docs/designs/proof-workbench/prototype`, `node checks/file-open-check.mjs` runs eight browser regressions for file opening, responsive feedback, keyboard focus, notation, unsupported input and board framing; `node checks/capture.mjs` exercises and captures the workflows. These checks need the existing `website/node_modules/playwright` installation; opening the artifact does not.
