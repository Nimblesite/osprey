# Osprey Native design system — "Luminous Flight"

Osprey Native looks like a modern flagship cross-platform UI framework — inspired by the clarity, product-first elegance, and airy visual polish of **flutter.dev** and **reactnative.dev**. Pure white daylight surfaces, soft atmospheric gradients, crisp elevated app screenshots and playable videos above the fold, and a clean dark mode that adapts automatically to `prefers-color-scheme`.

Start every page and demo with [the Osprey Native messaging guide](../../../docs/messaging/osprey-native.md). This design system is completely independent of the core Osprey language site (`#0c1325` navy / `#77d7f4` cyan) so Osprey Native can break out into its own repository cleanly.

## Brand mark & favicon

The Osprey Native mark (`src/assets/brand/logo.svg` and `src/assets/brand/favicon.svg`) is a clean geometric banking wing formed from two overlapping translucent planes — **Electric Native Blue** (`#0265dc`) and **FerroUI Violet** (`#6d28d9`) — symbolizing real native controls and GPU canvas rendering flying as one.

- Deployed as SVG favicon (`/assets/brand/favicon.svg`), 32×32 PNG (`/assets/brand/favicon-32.png`), 180×180 Apple Touch Icon (`/assets/brand/apple-touch-icon.png`), and `/favicon.ico` (generated at build time from `favicon.svg`).
- Navbar and footer render the mark beside plain-text **Osprey Native** — never a boxed text monogram.

## Color system (Light-first with automatic Dark Mode)

Declares `<meta name="color-scheme" content="light dark">` and `:root { color-scheme: light dark; }`, using `light-dark()` with `@media (prefers-color-scheme: dark)` fallbacks and `@media (forced-colors: active)` overrides.

| Role | Token | Light value | Dark value | Use |
| --- | --- | --- | --- | --- |
| Primary canvas | `--surface` | `#ffffff` | `#0d1117` | Main viewport & alternating white bands |
| Tinted band | `--surface-alt` | `#f4f7fb` | `#121821` | Soft alternating showcase bands |
| Elevated surface | `--surface-raised` | `#ffffff` | `#161d27` | Floating cards, code frames, blog cards |
| Code canvas | `--surface-code` | `#0f172a` | `#0a0e14` | Always-crisp dark code editor window |
| Primary ink | `--ink` | `#0f172a` | `#f0f6fc` | Display headings & primary text (≥14:1) |
| Secondary ink | `--ink-muted` | `#475569` | `#9fb0c7` | Subtitles, body copy, captions (≥5.7:1) |
| Subtle hairline | `--border` | `#e2e8f0` | `#263142` | Clean 1px section/card borders |
| Native Blue (Primary) | `--brand-native` | `#0265dc` | `#4da3ff` | Primary pill CTA, native-control highlights, links |
| FerroUI Violet | `--brand-canvas` | `#6d28d9` | `#a78bfa` | GPU canvas highlights, gradient accent pair |
| Signal Emerald | `--brand-signal` | `#059669` | `#34d399` | Signals, enforced accessibility, passing tests |

### Visual discipline

- **Product first, never boxes first:** real flagship app screenshots (Prism Synth/DAW, Osprey Agent, Windows Storage, Issue Inbox iOS/Android, Talon Bank, Sherbet Signal) and playable MP4 videos with posters and WebVTT captions carry the visual weight. Never showcase toy counter or boids demos on the homepage.
- **No pill spam or dashed borders:** use generous whitespace (`5.5rem`–`6.5rem` section padding), soft realistic elevation shadows (`0 24px 60px rgba(15, 23, 42, 0.10)`), rounded `16px–24px` media frames, and subtle blue-to-violet ambient glow behind device stages.
- **Accessible by default:** ≥4.5:1 text contrast, ≥3:1 non-text contrast, never color alone (WCAG 1.4.1), `:focus-visible` outlines with `3px` offset, `prefers-reduced-motion` respected, and zero horizontal overflow at `375px` and `1440px`.

## Typography & layout

- **Sans**: `"Plus Jakarta Sans", "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, system-ui, sans-serif`
- **Mono**: `"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace`
- **Display H1**: `clamp(2.6rem, 5.2vw, 4.35rem)`, weight `800`, tracking `-0.035em`, line-height `1.06`.
- **Section H2**: `clamp(1.85rem, 3.2vw, 2.75rem)`, weight `750`, tracking `-0.025em`, line-height `1.12`.
- **Container**: `1200px` max width (`75rem`), 8px spacing rhythm, `.prose`-prefixed editorial classes, CSS budget ≤ 1,800 LOC (target ~1,000 LOC), no Tailwind, content-hashed CSS/JS assets (`[WEBSITE-ASSET-VERSIONS]`).

## Page architecture

1. **Floating translucent navbar** with geometric Osprey Native SVG mark, concise links, and clean Light/Dark/Auto switcher.
2. **Product-first Hero (above the fold)**: centered headline with blue-to-violet gradient emphasis, 2-line pitch, rounded pill CTAs, and an overlapping **multi-device stage** showcasing **Prism Synth (macOS FerroUI)** flanked by **Issue Inbox on iOS (SwiftUI)** and **Issue Inbox on Android (Native Views)** over a soft luminous aura.
3. **Interactive Dual-Engine Showcase**: switch seamlessly between **Native Platform Controls** (Windows Storage WinUI, Osprey Agent SwiftUI, Talon Bank Web) and **FerroUI GPU Canvas** (Prism DAW playable video, Prism Spectral patch, Sherbet Signal GPUI).
4. **Playable App Films**: crisp 16:9 native `<video controls>` showcases for **Prism DAW** and **Osprey Agent** with posters and English WebVTT captions.
5. **Code-Beside-App + Syntax Flavor Tabs**: ReactNative.dev-style dark code window (`focusgroup="tablist nomemory"`, `hidden="until-found"`) paired with **Signals from the ground up** and **Talon Bank**.
6. **Accessibilitymaxxing & `@osprey/uitest`**: playable `@osprey/uitest` video beside a clean interactive Accessibility Tree Inspector.
7. **Framework Comparison Matrix**: responsive `@container` table comparing Osprey Native, Compose Multiplatform (CMP), Flutter, and React Native, plus 3 clean highlight columns explaining where Osprey Native pulls ahead of CMP.
8. **Editorial Blog Grid**: 3-column card grid on the homepage and dedicated `/blog/` archive.
