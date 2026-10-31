# Casting player design system

Design proposal · 1 November 2026 · [CAST-DESIGN]

A little music object: colourful, tactile and calm, with the delight of an old MP3 player and the familiarity of native controls. Open a file, choose a receiver, press Play. Safety, elegance and performance mean honest playback state, a small understandable interface, and restrained animation.

This document specifies intended behaviour, not implemented functionality. The images are generated visual concepts, not screenshots of working native controls. Skinning and video are design provisions; this task changes no application code. This intentionally playful product direction follows the user's brief rather than the website's visual palette.

## Delivery checklist [CAST-DESIGN-DELIVERY]

- [x] Generate and visually inspect three independent concepts.
- [x] Save all images beside this document.
- [x] Define native controls, interaction states, colour, spacing and motion.
- [x] Define skins without replacing native widgets.
- [x] Reserve a future video viewport.
- [x] Request peer feedback through TMC.
- [ ] Incorporate peer feedback when received.
- [ ] Verify the eventual implementation through native E2E journeys below.

## Visual directions [CAST-DESIGN-SKINS]

| Skin | Character | Reference |
| --- | --- | --- |
| Sherbet Signal | Warm ivory, peach, lilac; a luminous spinning disc | [Image](sherbet-signal.png) |
| Midnight Mint | Deep navy, mint and lavender; a compact hi-fi deck | [Image](midnight-mint.png) |
| Poolside Pop | Powder blue, cobalt, tangerine and lemon; graphic portable-player joy | [Image](poolside-pop.png) |

![Sherbet Signal](sherbet-signal.png)

![Midnight Mint](midnight-mint.png)

![Poolside Pop](poolside-pop.png)

Sherbet Signal is the initial art direction; Midnight Mint and Poolside Pop are equally supported skin concepts. The images explore different compositions. Production skins share the same responsive layout and control positions: the wide Midnight composition is a window-size adaptation, not a skin-specific rearrangement.

Image review corrections: use platform widgets rather than reproducing the illustrated glossy Pause button; increase muted-text contrast; use a receiver icon for a receiver, not the phone depicted in Poolside; remove Poolside's redundant casting icon. Decorative waves must never imply measured audio amplitude. The generated images do not demonstrate animations, accessibility or platform fidelity.

## Foundation tokens [CAST-DESIGN-TOKENS]

| Semantic role | Sherbet Signal | Midnight Mint | Poolside Pop |
| --- | --- | --- | --- |
| Canvas | `#FFF8EF` | `#10192B` | `#D9EEFF` |
| Primary ink | `#252333` | `#F4F7FF` | `#102958` |
| Secondary ink | `#625968` | `#BAC6D9` | `#36557A` |
| Action accent | `#A83D22` | `#A8EED4` | `#174EC6` |
| On accent | `#FFFFFF` | `#10192B` | `#FFFFFF` |
| Decorative colours | `#FFAA86`, `#D8C3EF`, `#FFE4A1` | `#B4A5FF`, `#70B9C4` | `#FF7828`, `#FFD44F` |

These are proposed source tokens; final widget rendering must pass contrast checks on each OS. Pastel illustration colours are not text or focus colours. Error, focus, selection, disabled and high-contrast appearances use platform semantic colours. Never encode connection or playback state through colour alone.

Use the platform system font: SF on Apple platforms, Segoe UI on Windows, the Android system face on Android. Body 15–16 logical units, secondary labels 13–14, track title 24–28 semibold, times 13 with tabular digits. Respect system text scaling and allow track titles to wrap to two lines; expose the full filename accessibly.

Spacing scale: 4, 8, 12, 16, 24, 32 logical units. Desktop content inset 24; phone inset 16. Illustrations may use 16-unit corners. Native buttons, menus and sliders keep OS geometry, focus rings and minimum sizing. Target 44-point touch regions on iOS and 48-dp regions on Android; never shrink labels to squeeze in controls.

## One stable composition [CAST-DESIGN-LAYOUT]

The vertical order is destination, media stage, track and state, timeline, transport, volume and speed, then Open file. Artwork is the expressive region; transport stays visually quiet and immediately recognizable.

Start around 520 × 640 logical units on desktop, with an adaptable minimum near 360 × 520. Shrink the artwork before the controls. At narrow widths, stack supporting controls and allow vertical scrolling at large text sizes. A wide window can place artwork beside metadata and controls without changing keyboard order. These are layout targets to verify, not fixed window constraints.

The media stage has a 16:9 outer slot. Audio artwork fits within it, preserving its aspect ratio: a circular disc can remain circular inside this region. The images' larger artwork is exploratory; the implemented stage yields space to controls and text first. See [CAST-DESIGN-VIDEO].

## Native control contract [CAST-DESIGN-NATIVE]

Compose the application with `@osprey/native` nodes from `packages/native`; use real platform buttons, sliders, menus, file dialogs and accessibility objects through those adapters. Osprey owns state and decisions; the native host owns widget rendering and platform integration. Native host capabilities must be verified before implementation; this proposal does not assert every adapter exists.

| Element | Behaviour |
| --- | --- |
| Destination | Native menu showing receiver name and connection state; discovery errors remain visible with Retry |
| Play / Pause | One native button with changing label and icon; requests are pending until receiver acknowledgement |
| Stop | Separate native button; stop receiver playback, retain the selected file, reset confirmed stopped position to zero |
| Timeline | Native seek slider with elapsed and duration labels; disabled until duration and seeking capability are known |
| Volume | Separately labelled native slider controlling the selected output; show unavailable when receiver volume cannot be controlled |
| Speed | Native popup with supported rates only; desired choices are 0.75×, 1×, 1.25×, 1.5× and 2× |
| Open file… | Native single-file picker; preserves current selection when cancelled |
| Appearance | Native application/view menu or settings entry for skin selection; no permanent extra toolbar |

Prefer native symbols with accessible names over custom icons. Platform title bars remain platform-owned. Use standard keyboard activation and slider key handling. Space may toggle playback only when focus is outside a control that owns that key; platform Open shortcut invokes the file picker. Never steal screen-reader or system shortcuts.

Playback speed is capability-gated. An unsupported receiver shows disabled “Speed unavailable on this device”; it must never display an unconfirmed applied rate. The existing [Chromecast package](../../../packages/chromecast/README.md) documents casting and media status, not proof of all controls proposed here. Treat pause, stop, seeking, volume and speed as integration requirements until verified against real devices.

## File and receiver journeys [CAST-DESIGN-STATE]

| State | Visible result and permitted actions |
| --- | --- |
| Empty | “Drop an audio file” and Open file; no invented duration; transport unavailable |
| Drag over | Thin accent outline and “Drop to open”; underlying controls stay visible |
| Validating | Filename and “Checking file…”; retain the previous valid selection until validation succeeds |
| Ready | Track, known duration, destination and Play; opening a file alone never starts sound |
| Connecting | Named destination and “Connecting…”; cancel available; no claim that playback has begun |
| Playing | Confirmed receiver state, Pause and Stop; seek, volume and speed only if supported |
| Paused | Play resumes confirmed position; decorative animation is stationary |
| Stopped / ended | Play starts the selected file again; distinguish stopped from naturally ended status |
| Disconnected | “Connection lost”; preserve track and last confirmed position, offer reconnect; receiver may still be playing |
| Failure | Plain cause and recovery near the affected control; keep the last valid state, never fake success |

File picker and drop share validation. Accept one readable supported audio file; reject directories, unsupported or corrupt files, and multiple-file drops with a useful message. Dropping a replacement during playback validates it first, then asks explicitly whether to replace the current track; cancellation leaves playback intact. Picker cancellation is not an error. File permissions and sandbox access belong to the host.

Receiver selection does not silently send media to another room. While playing, selecting another receiver requires an explicit transfer action; keep the current destination until the handoff succeeds, and expose a failed stop on the old receiver. Discovery, load, transport and acknowledgement failures are typed/handled outcomes, following [FAILURE-EXPLICIT]. Disconnected does not mean stopped.

Scrubbing previews a time locally and sends a seek on release; the acknowledged position becomes authoritative. Clamp to known bounds and restore confirmed position on failure. Show pending operations; prevent repeated conflicting requests without freezing unrelated controls. Do not show codec or infrastructure details unless they explain a recovery action.

## Skins are data [CAST-DESIGN-SKIN-CONTRACT]

Bundle the three initial skins. A skin supplies an ID, display name, schema version, semantic palette, bundled artwork and an approved decorative-motion preset. It cannot provide code, network resources, custom controls, arbitrary layout, hit regions or accessibility labels. Do not build a skin marketplace or importer in this first scope.

Changing skin keeps the track, receiver, time, volume, speed, focus and playback state. Apply colour atomically and crossfade artwork for 180 ms; never reload media. Persist the selected skin locally. On an invalid skin, use a validated bundled fallback and disclose that the chosen skin could not load. Unknown schema versions are rejected rather than partly applied.

Support OS light/dark preferences through validated variants, with an explicit user override; the names are aesthetic choices, not forced OS modes. High contrast overrides decorative palettes and removes translucency. Reduced motion overrides all skin motion. Native widgets may accept semantic tint where supported, but their exact geometry and colour rendering will differ by platform. This is expected.

## Motion vocabulary [CAST-DESIGN-MOTION]

| Trigger | Motion | Bound |
| --- | --- | --- |
| Playing audio | Slow disc rotation, or gentle signal/wave drift per skin | One decorative loop; disc revolution around 24 seconds |
| Pause / stop | Ease decoration to stillness | 180 ms; never delays the command |
| Drag enter / leave | Outline opacity | 120 ms; no layout movement |
| Track change | Artwork and title crossfade | 180 ms; controls stay anchored |
| Connect | Native progress indicator | Only while an operation is pending |
| Skin change | Artwork crossfade | 180 ms; no flashing background |

Use platform animation facilities. No bouncing transport buttons, sound effects, animated text or fabricated spectrum readings. Stop decorative rendering when the window is hidden or minimized. Reduced motion presents final states immediately and replaces continuous decoration with a static image. A waveform is decorative unless sourced from actual audio data.

## Future video slot [CAST-DESIGN-VIDEO]

Reserve the media stage now; do not add inactive video buttons or claim video support. Later the same slot can host a native video surface in place of the audio illustration, with aspect-fit letterboxing and no cropping. Skins never tint, filter, mask or draw over decoded video or captions.

Transport, destination, timeline, volume and supported speed stay in the same component tree and order. Distinguish “Playing on Living room” from a local preview; do not imply a preview exists when the receiver is the only output. Local playback, remote casting, codec support and remote preview are separate capabilities.

When video is implemented, add captions, fullscreen and picture-in-picture only for supported platform/media combinations. Keep captions inside the video safe region and controls outside it in the default layout. On phones, use native document selection and share/open integration; dragging is an optional convenience. Landscape video can expand the stage while preserving accessible transport access.

## Implementation acceptance [CAST-DESIGN-ACCEPTANCE]

Future native E2E tests should cite the relevant IDs above and exercise complete journeys with multiple assertions. No application implementation or runtime testing is claimed by this design handoff.

- [CAST-DESIGN-STATE]: Open, cancel, drop valid/corrupt/unreadable/multiple files, replace while playing, cancel replacement, recover from discovery/load failures; assert previous state survives failures.
- [CAST-DESIGN-NATIVE]: Cast, pause, resume, seek, stop and replay on supported receivers; verify acknowledgement, unsupported controls, rejected commands and reconnect without false success.
- [CAST-DESIGN-SKIN-CONTRACT]: Change all three skins while playing and paused; retain focus, output, time and rate; test invalid schema, restart persistence, high contrast and reduced motion.
- [CAST-DESIGN-LAYOUT]: Resize desktop windows and use maximum text scaling; verify full keyboard traversal, accessible names/values and no clipped controls with VoiceOver and Narrator.
- [CAST-DESIGN-VIDEO]: When implemented, replace artwork with portrait and landscape video; assert aspect preservation, captions, native fullscreen, receiver-only state and platform capability handling.

## Provenance and review [CAST-DESIGN-REVIEW]

Created with the built-in image generation tool. Exact generation prompts are in [prompts.md](prompts.md). The three PNG files are visual references, not production widget assets. Original generations remain in the Codex image archive; these project copies are authoritative for this proposal.

Peer review requested through TMC from the other active agent. Feedback is pending; no peer endorsement is claimed.
