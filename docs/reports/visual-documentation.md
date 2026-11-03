# Visual documentation refresh [DOCS-VISUAL]

## Checklist

- [x] Read messaging and design system; inspect the live Native page.
- [x] Register TMC as OspreyCodexDocs and coordinate file ownership.
- [x] Replace the Native text wall with real Windows, macOS, iOS, Android and browser captures.
- [x] Record Osprey Agent and produce its narrated film through the TikTok factory.
- [x] Supply captions, chapters, poster, transcript and a dedicated agent watch page.
- [x] Measure delivered video durations for VideoObject and video sitemap metadata.
- [ ] Reduce tracked documentation prose by at least 5%.
- [ ] Add useful media to long guides, targeting one per 500 words.
- [ ] Verify rendered docs, media playback, local links and phone/desktop layout.
- [ ] Commit verified batches and clean safe temporary production files.

## Evidence

Baseline: 29,735 prose words across tracked Markdown pages in `website/src/docs`, captured before this refresh. Excludes front matter, fenced code and template invocations. New media captions and transcripts are useful instructional content, not candidates for mechanical deletion. Preserve all security qualifications and feature limits.

The Native showcase uses existing WinUI test goldens and mobile simulator/emulator captures, plus fresh Prism Synth and Talon Bank captures. `website/src/assets/images/native/README.md` records provenance. No platform screenshot is a mockup; no screenshot promises universal host conformance.

The agent film is produced under `TikTok/projects/osprey/videos/agent-native/` with the shared native recorder, local licensed Kokoro narration, score, renderer and delivery checks. It navigates an existing real calculator conversation without provider calls. The public crop excludes the project sidebar; two local traceback paths are redacted. Test failures, assertions, successful verification and the unsandboxed warning remain visible. Delivery: 1920 × 1080, H.264/AAC, approximately 80 seconds. The factory's 24 tests pass.

Google's [video indexing guidance](https://developers.google.com/search/docs/appearance/video) and [VideoObject guidance](https://developers.google.com/search/docs/appearance/structured-data/video) call for visible videos, dedicated watch pages, stable fetchable thumbnails/content, descriptive metadata, original upload dates and accurate durations. Existing shared players already emit most of this; the refresh adds measured MP4 durations and the agent watch page. There is no Google rule prescribing one image per 500 words; 500 is this project's editorial target. Search indexing cannot be guaranteed by markup.

Metadata regression was red against chapter-derived duration and passes with measured MP4 metadata. Refresh measurements with `make -C website video-metadata` after changing video assets. Original upload dates are preserved.
