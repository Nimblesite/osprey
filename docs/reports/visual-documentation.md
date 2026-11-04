# Visual documentation refresh [DOCS-VISUAL]

## Checklist

- [x] Read messaging and design system; inspect the live Native page.
- [x] Register TMC as OspreyCodexDocs and coordinate file ownership.
- [x] Replace the Native text wall with real Windows, macOS, iOS, Android and browser captures.
- [x] Record Osprey Agent and produce its narrated film through the TikTok factory.
- [x] Supply captions, chapters, poster, transcript and a dedicated agent watch page.
- [x] Measure delivered video durations for VideoObject and video sitemap metadata.
- [x] Reduce tracked documentation prose by at least 5% (28,131 → 25,735 words, an 8.52% reduction).
- [x] Add useful media to long guides, targeting one per 500 words.
- [x] Verify rendered docs, media playback, local links and phone/desktop layout.
- [x] Commit verified batches and clean safe temporary production files.

## Evidence

Baseline: 28,131 prose words across tracked Markdown pages in `website/src/docs` at `17e984159beedb7bcdec1f7e11164540749db245`. Counts Markdown inline text and inline code; excludes front matter, fenced code, link destinations and template invocations. New media captions and transcripts are useful instructional content, not candidates for mechanical deletion. Preserve all security qualifications and feature limits.

The Native showcase uses existing WinUI test goldens and mobile simulator/emulator captures, plus fresh Prism Synth and Talon Bank captures. `website/src/assets/images/native/README.md` records provenance. No platform screenshot is a mockup; no screenshot promises universal host conformance.

The agent film is produced under `TikTok/projects/osprey/videos/agent-native/` with the shared native recorder, local licensed Kokoro narration, score, renderer and delivery checks. It navigates an existing real calculator conversation without provider calls. The public crop excludes the project sidebar; two local traceback paths are redacted. Test failures, assertions, successful verification and the unsandboxed warning remain visible. Delivery: 1920 × 1080, H.264/AAC, approximately 80 seconds. The factory's 24 tests pass.

Google's [video indexing guidance](https://developers.google.com/search/docs/appearance/video) and [VideoObject guidance](https://developers.google.com/search/docs/appearance/structured-data/video) call for visible videos, dedicated watch pages, stable fetchable thumbnails/content, descriptive metadata, original upload dates and accurate durations. Existing shared players already emit most of this; the refresh adds measured MP4 durations and the agent watch page. There is no Google rule prescribing one image per 500 words; 500 is this project's editorial target. Search indexing cannot be guaranteed by markup.

Metadata regression was red against chapter-derived duration and passes with measured MP4 metadata. Refresh measurements with `make -C website video-metadata` after changing video assets. Original upload dates are preserved.

The Python and backend guides now quote their fallible Tensor declarations and GPU arms from implementation source. Two browser regressions were red against the removed abort-helper examples and pass with source-derived excerpts. The first-app tutorial now checks without unused-binding warnings.

## Final verification

- Node 24 website CI pipeline (`npm test -- --workers=2`): 308 browser tests passed, including the native/Wasm prebuild and both Studio flavors. Six targeted tests passed again after the final build: long-guide media density, current fallible Tensor examples, delivered video durations, and Native playback/image decoding/layout at 390 and 1440 pixels.
- All 17 authored guides with at least 500 prose words meet the media target. The browser check excludes video-card posters, decodes every illustration and checks overflow. Short reference entries and generated function pages are outside this editorial metric.
- All 20 delivered video durations match browser metadata; VideoObject preserves millisecond precision. Captions, transcripts, chapter tracks and stable posters remain available. Original upload dates are unchanged.
- 1,595 local links and anchors checked across changed guides: no broken targets. JavaScript syntax checks and `git diff --check` pass.
- Website-scope CI contracts pass: CI scope, benchmark publication, instruction gate and branch protection (two active rulesets, one required CI check). This verifies the documentation patch; concurrent compiler/runtime work is not certified by this report.
- Kimi authentication succeeded but the review call hit its monthly quota. The requested fallback, GLM 5.3, completed a read-only review; native-versus-web engine scope, app-specific host boundaries and historical Mathlib evidence were clarified from its findings.
- The first-app example runs twice with the expected saved state; host and Metal matrix examples both return 7.0. Wasm guidance now uses the current ARC default. Package screenshots retain their visible audit-unverified status, and agent captures retain the unsandboxed warning.

Temporary documentation export, hunt reports and exported XCTest attachments were removed after captures were copied. The final local site is retained for review; shared compiler caches and other agents' build products are preserved. Source commits are pushed without triggering a website release: production publication still follows the repository's release/manual-deploy workflow.
