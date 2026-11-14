# Osprey Agent — YouTube upload

Prepared 2026-11-14 for the 80.4-second live recording.

## Files

- Video: `../../src/toolchain/website/src/assets/videos/osprey-agent-native.mp4`
- Thumbnail: `../../src/toolchain/website/src/assets/videos/osprey-agent-native-youtube-thumbnail.png`
- Captions: `../../src/toolchain/website/src/assets/videos/osprey-agent-native.en.vtt`

The MP4 is materialized locally and tracked in Git LFS: H.264 video, AAC audio, 1920 × 1080, 11,646,650 bytes.
The new thumbnail is 1672 × 941, 1,579,299 bytes, generated with the built-in image tool from the supplied thumbnail style reference and the existing video's app poster.
The video and poster were re-shot live on 2026-11-14: every frame is the real app while DeepSeek fixes the staged calculator.

## Title

AI Coding Agent Demo: Fixing Code in a Native macOS App | Osprey

## Description

Watch Osprey Agent, an AI coding agent in a native macOS app, inspect failing calculator tests, review a code edit and show the passing results.

The app uses native SwiftUI controls and Osprey application logic compiled ahead of time through LLVM; this video records a live run with real tool outputs.

Explore Osprey: https://www.ospreylang.dev/
Source code: https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/agent

Osprey is actively evolving; the agent is not sandboxed and can run commands or change files with your account's permissions.

#AICodingAgent #Osprey #NativeApps

## Keyword evidence

Google Trends, checked 2026-11-14: Worldwide, past 12 months, YouTube Search, all categories, search terms rather than topics.
[Open the comparison](https://trends.google.com/trends/explore?date=today%2012-m&gprop=youtube&q=AI%20coding%20agent,coding%20agent,Python%20GUI,Pydantic%20AI,algebraic%20effects&hl=en).

| Search term | Average relative interest in this comparison |
|---|---:|
| coding agent | 56 |
| Python GUI | 29 |
| AI coding agent | 28 |
| Pydantic AI | 3 |
| algebraic effects | 0 |

These are sampled, normalized interest indexes, not search counts; scores from separately scaled comparisons must not be compared directly.
The title leads with the relevant measured phrase **AI coding agent**, which also contains **coding agent**.
The thumbnail uses **AI CODING / AGENT / REAL TOOLS**.
Native macOS and Osprey describe the actual film; they are not claimed here as measured keywords.
Do not market this recording as a local/offline LLM demo, a live model run or a build tutorial: it shows an existing conversation in the real SwiftUI app, and no provider calls were made during filming.
The voiceover's old launch path has since moved; the current entry point is `make -C src/toolchain/targets/macos agent PROJECT=/path/to/project`.

## Review

The captions and original Videos project narration/README establish the content and recording provenance.
A bounded, read-only `zai/glm-5.3-flash` review found no factual errors and noted possible ambiguity in “Demo” and “REAL TOOLS.”
Parent review retained those terms because the tools/results are real and the run is filmed live.
The generated thumbnail was visually checked for readable exact headline text, the correct native app subject and the requested white/cyan/yellow style.
No new footage or YouTube upload was made.

## Generation prompt

Built-in image generation; first reference = supplied Chromecast thumbnail style, second reference = `osprey-agent-native-poster.jpg`.

```text
Use case: ads-marketing.
Asset type: a finished YouTube thumbnail, full-bleed landscape 16:9, for an ALREADY RECORDED 80-second Osprey coding-agent video.
Input image 1 is STYLE REFERENCE ONLY: imitate its bold condensed headline, white/cyan/yellow hierarchy, dark navy/purple background and neon cyan glow. Do not include the outer YouTube interface, upload date, runtime badge or Chromecast imagery.
Input image 2 is the ACTUAL APP SCREEN reference: incorporate its central dark SwiftUI agent window into a large laptop display on the right. Preserve the visible calculator code, chat layout and app identity as accurately as possible; remove the surrounding video title/caption area. This is a recorded conversation about failed tests, a code edit and passing tests.
Composition: left half huge beautifully typeset condensed uppercase text, right half a premium dimensional open laptop with actual app screenshot in its display; top right two simple luminous status tiles, a red cross with FAIL then a cyan arrow to a green check with PASS. Small turquoise Osprey bird and white Osprey wordmark bottom left, following the provided brand reference. Subtle cyan energy arc across the bottom and restrained purple rim lighting, dramatic contrast and clarity even at small thumbnail size.
Main text exactly, in three blocks: "AI CODING" (white), "AGENT" (electric cyan, exceptionally large), "REAL TOOLS" (black bold italic on bright yellow angled banner).
Other permitted text: "FAIL", "PASS", "Osprey".
Constraints: only these exact headline words; no robot, no face, no unrelated logos, no local/offline/free claims, no subscription claims, no YouTube UI or duration badge. Keep the bottom-right corner uncluttered for YouTube's own duration badge. Fill the entire frame with the design, no outer margins or black bars. The result should feel like the same polished thumbnail series as reference1, with the native coding agent as the new subject.
```
