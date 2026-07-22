# Osprey Blog Header Images — Image-Generation Design System

**This document is a brief for an image-generation AI.** Hand it over verbatim. It defines exactly how to produce header images for the Osprey blog so every image is on-brand, consistent, and drops cleanly into the site.

One generated image per post → used at three crops (hero banner, 16:9 card, 1:1 thumbnail). **Post titles are NOT baked into the image** — the website overlays the title in HTML. Your job is the artwork only.

---

## 1. Brand North Star

Osprey is a modern **functional programming language** with typed algebraic effects, lightweight fiber concurrency, immutable persistent collections, pattern matching, and LLVM code generation. The brand feeling is **precise, engineered, nocturnal, premium, high-contrast, calm**.

Three non-negotiable threads must appear in **every** image:

1. **Midnight slate canvas** — near-black, deep navy-blue background. Never light, never gray, never pure black.
2. **A single cyan accent** — electric ice-cyan `#77D7F4` is the *only* vibrant color. It carries all the energy. Everything else is dark or near-white.
3. **The Osprey thread** — the brand is an **osprey (fish-eagle) in flight**: ascent, motion, predatory precision. Either show a stylized osprey/wing/talon motif, or evoke flight and upward diagonal energy.

> If an image could belong to any generic tech company, it has failed. It must read as **Osprey**.

---

## 2. The Look — Primary Art Direction

**"Luminous Architecture on Midnight."** Abstract, geometric, depth-layered. The subject is always a **glowing cyan structure** — a network of nodes and edges, a branching data-structure lattice, parallel light-threads, or a stylized wireframe osprey — suspended in deep midnight space, lit by **directional cyan rim-light and soft glow** (never drop shadows).

- **Style:** clean digital 3D / abstract geometric render. Minimalist. Generous negative space. High contrast. Subtle glassmorphism (frosted translucent panels with thin bright edges). A faint dotted/grid texture may sit far in the background.
- **Lighting:** dramatic, directional. Cyan light rims the edges of forms; soft cyan bloom/glow radiates from focal points. The background falls off to near-black at the corners (vignette). **Glow, not shadow.**
- **Depth:** built from tonal layers and thin 1px luminous outlines, not black shadows.
- **Mood:** quiet, futuristic, disciplined, expensive.

### Recurring motifs (pick ONE focal idea per image)
- **Node constellations** — connected points + edges, like an effect/data-flow graph.
- **Branching lattice / trie** — a glowing 32-way branching tree (great for data-structure topics); old branches stay lit (structural sharing).
- **Parallel light-threads** — fibers as luminous strands running in parallel, weaving.
- **Carved code panel** — a frosted-glass slab with abstract mono code *as texture* (never readable words), cyan-lit.
- **Wireframe osprey / wing / talon** — the mascot rendered in cyan wireframe or light-streaks, mid-flight.

---

## 3. Color & Light (exact values — do not drift)

| Role | Hex | Use |
|------|-----|-----|
| Base canvas (corners) | `#070D1F` | Darkest background, vignette falloff |
| Background mid | `#0C1325` | Main background field |
| Surface / panels | `#0D1836` | Frosted slabs, raised forms |
| Code-panel surface | `#0D152B` | Carved code texture background |
| **Primary accent (cyan)** | `#77D7F4` | **The hero color** — structures, rim-light, glow |
| Bright highlight | `#BDEEFF` | Hottest highlights, light cores |
| Cyan glow wash | `rgba(119,215,244,0.10)` | Atmospheric bloom, faint fills |
| Text-white (if any UI dots) | `#DCE1FB` | Near-white ice |
| Muted detail | `#BDC8CD` / `#3E484C` | Faint secondary lines |
| Periwinkle (rare 2nd tone) | `#BBC5EC` | Optional cool secondary, sparingly |
| Warm amber (RARE accent) | `#FFBE65` | At most a *single tiny* spark, optional, never dominant |

**Forbidden colors:** purple/magenta, teal-green, neon green, hot orange fields, pastels, warm sunset palettes, pure white backgrounds, pure `#000000`. The palette is **midnight blue + cyan**, full stop.

---

## 4. Typography-in-Image Policy

- **Do NOT render any real text, titles, or readable words.** The site overlays the post title in HTML.
- Code may appear **only as abstract texture** — blurred, fragmented, or low-contrast mono glyphs that suggest code without spelling anything. Never let the model attempt real sentences (it produces garbled gibberish).
- No logos with text, no watermarks, no UI chrome with labels.

---

## 5. Output Specs & Crops

Generate **one master image per post at `1600 × 840` (≈1.9:1)**, then the site crops it. Design so all three crops work:

| Crop | Aspect | Pixels | Where used |
|------|--------|--------|------------|
| Master / hero banner | ~1.9:1 | 1600 × 840 (export) | Featured post, OG/social |
| Standard card | 16:9 | center crop | Blog grid cards |
| Thumbnail | 1:1 | center-square crop | Mobile list thumbnails |

**Composition rules for safe cropping:**
- Keep the **focal element centered or center-left**, fully inside the central square — so the 1:1 crop never loses it.
- Keep **one side (preferably the upper-left or upper-third) visually quiet** (negative space / soft falloff) so an overlaid title stays legible.
- No important detail in the outer 8% margins (it gets cropped).

---

## 6. The Reusable Prompt Template

Fill the slots. Keep this exact order and spirit.

```
[SUBJECT/MOTIF], abstract geometric digital render, glowing electric cyan (#77D7F4)
[STRUCTURE] suspended in deep midnight-blue space (#070D1F to #0C1325),
[COMPOSITION: focal placement + negative space side],
directional cyan rim-light and soft bloom glow, thin luminous outlines, subtle
glassmorphism, faint background dot-grid, high contrast, vignette falloff to near
black, minimalist, premium, futuristic, no text, no words, 1.9:1 wide banner,
cinematic depth.
NEGATIVE: [see §9]
```

### Filled examples
**A — Node constellation:**
> A luminous network of connected nodes and edges, abstract geometric digital render, glowing electric cyan (#77D7F4) points linked by thin glowing lines, suspended in deep midnight-blue space (#070D1F to #0C1325), focal cluster centered with open negative space upper-left, directional cyan rim-light and soft bloom, glassmorphism, faint background dot-grid, high contrast, vignette to near black, minimalist, premium, futuristic, no text, no words, 1.9:1 wide banner, cinematic depth.

**B — Wireframe osprey:**
> A stylized osprey in flight composed of glowing cyan wireframe and light-streak trails, wings spread mid-ascent, abstract geometric render, electric cyan (#77D7F4) on deep midnight-blue (#070D1F to #0C1325), bird centered-left with quiet negative space upper-right, dramatic directional cyan rim-light, soft glow, thin luminous outlines, faint dot-grid, high contrast, vignette, minimalist, premium, no text, no words, 1.9:1 wide banner, cinematic depth.

---

## 7. Per-Post Prompts (ready to paste)

Save each output to the path shown. Same basename → it slots straight into the site.

### `building-web-apis-with-pattern-matching` → `src/assets/images/blog/building-web-apis-with-pattern-matching.png`
*"Building Type-Safe Web APIs with Osprey's Pattern Matching"*
> Abstract geometric digital render of incoming data-flow paths converging into a glowing cyan decision-lattice — multiple inbound light-streams sorted into clean exhaustive branches, each branch capped with a small glowing cyan node, representing pattern matching routing requests. Electric cyan (#77D7F4) structure suspended in deep midnight-blue space (#070D1F to #0C1325), focal lattice centered, quiet negative space upper-left, directional cyan rim-light, soft bloom, glassmorphism, thin luminous outlines, faint dot-grid, high contrast, vignette to near black, minimalist, premium, futuristic, no text, no words, 1.9:1 wide banner, cinematic depth.
> NEGATIVE: *(shared, §9)*

### `the-memory-safe-revolution` → `src/assets/images/blog/the-memory-safe-revolution.png`
*"The Memory-Safe Revolution"*
> Abstract geometric digital render of a glowing cyan protective shield-lattice formed from interlocking hexagonal cells around a secure luminous core, with a faint wireframe osprey silhouette ascending behind it, conveying memory safety and a secure future. Electric cyan (#77D7F4) on deep midnight-blue (#070D1F to #0C1325), focal shield centered, open negative space upper-right, dramatic directional cyan rim-light, soft glow, thin luminous outlines, glassmorphism, faint dot-grid, high contrast, vignette, minimalist, premium, futuristic, no text, no words, 1.9:1 wide banner, cinematic depth.
> NEGATIVE: *(shared, §9)*

### `persistent-collections` → `src/assets/images/blog/persistent-collections.png`
*"Persistent Collections: Immutable List and Map with Structural Sharing"*
> Abstract geometric digital render of a glowing cyan branching tree / 32-way trie growing upward, where a new bright branch forks off while the older branches remain fully lit and intact — visualizing immutable structural sharing. Electric cyan (#77D7F4) lattice suspended in deep midnight-blue space (#070D1F to #0C1325), focal tree centered, quiet negative space upper-left, directional cyan rim-light, soft bloom, thin luminous outlines, glassmorphism, faint background dot-grid, high contrast, vignette to near black, minimalist, premium, futuristic, no text, no words, 1.9:1 wide banner, cinematic depth.
> NEGATIVE: *(shared, §9)*

These three must feel like **one family**: identical palette, lighting, and negative-space discipline; only the central structure changes.

---

## 8. (reserved)

---

## 9. Canonical Negative Prompt

Append to every prompt:

```
text, words, letters, captions, watermark, logo with text, UI labels, code you can read,
gibberish text, people, faces, hands, animals (except stylized osprey), photographic,
stock photo, realistic photo, clutter, busy, low contrast, washed out, pastel, purple,
magenta, teal green, neon green, rainbow, warm sunset, orange background, pure white
background, pure black, gray flat background, drop shadow, harsh shadows, bevel, skeuomorphic,
3d cartoon mascot, cute, childish, blurry, jpeg artifacts, noise, oversaturated, frame, border
```

---

## 10. Model-Specific Cheat-Sheets

**Midjourney v6+**
- Append `--ar 16:9 --style raw --stylize 150`. (Use `--ar 1.91:1` if available for the master.)
- Lock a **single `--seed N`** across all three posts to keep the family coherent; vary only the subject text.
- Use `--no text, people, purple, green` as a backup to the negative list.

**DALL·E 3 / GPT-Image**
- It ignores `--flags` and weights — feed the prompt as **natural language**, and explicitly say *"absolutely no text, words or letters anywhere in the image."*
- Ask for "wide 16:9 banner." Generate 2–3, pick the cleanest. It is the worst at avoiding text — reject any output with glyphs.

**Stable Diffusion / Flux**
- Put the §9 list in the **negative prompt** field.
- Flux: prompt-follows-language well; SDXL: add quality tags `highly detailed, sharp, volumetric light, octane render`.
- Use a **fixed seed + same sampler/steps** across the three for family consistency. CFG ~5–7. Sampler: DPM++ 2M Karras (SDXL) / default (Flux).

---

## 11. Consistency & Iteration

**Keep the set in-family**
- Same palette hexes, same lighting model, same vignette, same negative-space rule — every time.
- Fixed seed where the model supports it; change only the central motif.
- One focal structure per image. Resist clutter.

**Acceptance checklist (all must be true):**
- [ ] Background is midnight blue, falling to near-black at corners.
- [ ] Cyan `#77D7F4` is the only vibrant color.
- [ ] One clear focal structure, centered enough to survive a 1:1 crop.
- [ ] One side is quiet negative space for the overlaid title.
- [ ] **Zero readable text / no garbled glyphs.**
- [ ] Reads unmistakably as Osprey (flight/precision/engineered).
- [ ] Glow, not drop-shadow. High contrast. Clean, not busy.

**Reject if:** any text appears • palette drifts (purple/green/warm) • background is light/gray/pure-black • a person/face/hand appears • cluttered or low-contrast • the focal element is lost in a square crop.

---

## 12. Optional Variants

- **Carved code panel:** swap the focal structure for a frosted-glass slab of abstract, unreadable mono code lit by cyan edge-light — good for tooling/compiler posts.
- **Parallel light-threads:** luminous cyan strands running in parallel and gently weaving — good for concurrency/fiber posts.
- **Single tiny warm spark:** one small `#FFBE65` glint as a focal accent is permitted *rarely* — never more than a spark, never a field.

---

### File handoff

Output PNG (or WebP), master ~`1600×840`, sRGB. Name each file exactly `<post-slug>.png` and place in `website/src/assets/images/blog/`. The site references that path; matching the basename means your generation slots in with **zero code changes**.
