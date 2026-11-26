# Osprey Native design system — "Drafting Studio"

Osprey Native looks like an architectural drafting studio: crisp daylight paper and deep carbon graphite, engineered hairlines, tactile surfaces, and two distinct accent channels that mirror the framework's core promise — **Native Controls** and **FerroUI GPU Canvas** — without forcing a choice between them.

Start every page and demo with [the Osprey Native messaging guide](../../../docs/messaging/osprey-native.md). This design system is intentionally independent of the core Osprey language site (`#0c1325` navy / `#77d7f4` cyan) so Osprey Native can break out into its own repository with a distinct visual identity.

## Color and surface architecture

Osprey Native ships both **light** and **dark** modes from the first byte (`<meta name="color-scheme" content="light dark">` and `:root { color-scheme: light dark; }`), using CSS `light-dark()` with a `@media (prefers-color-scheme: dark)` fallback and explicit `@media (forced-colors: active)` system-color rules.

| Role | Token | Light value | Dark value | Use |
| --- | --- | --- | --- | --- |
| Page canvas | `--surface` | `#f7f5f0` | `#111317` | Full viewport background |
| Elevated surface | `--surface-raised` | `#ffffff` | `#181c22` | Cards, panels, showcase frames |
| Inset surface | `--surface-inset` | `#edeae2` | `#0b0d10` | Code blocks, accessibility trees, subtle wells |
| Primary ink | `--ink` | `#14171d` | `#f2f4f7` | Headings, body copy, primary controls (≥12:1) |
| Secondary ink | `--ink-muted` | `#4a5260` | `#a8b0be` | Captions, metadata, secondary descriptions (≥5.8:1) |
| Hairline border | `--border` | `#c8c4b8` | `#343b46` | Structural dividers and card boundaries |
| Interactive border | `--border-strong` | `#747b86` | `#808998` | Inputs, buttons, focusable wrappers (≥3:1 non-text) |
| Primary accent (GPU Canvas) | `--accent-canvas` | `#b83915` | `#ff7a4d` | Primary CTA, FerroUI / GPU Canvas badges, active tab underlines (≥4.5:1) |
| Secondary accent (Native) | `--accent-native` | `#184fb8` | `#6ea8fe` | SwiftUI / WinUI / Android badges, native-mode highlights (≥4.5:1) |
| Verified accent (A11y / Tests) | `--accent-verified` | `#0d6e4b` | `#3dd69d` | Accessibility tree nodes, passing `@osprey/uitest` assertions, signals (≥4.5:1) |
| On-accent ink | `--on-accent` | `#ffffff` | `#111317` | Text on filled primary CTA buttons |

### Accessibility color rules

- **Never use color alone** (WCAG 1.4.1): every status, rendering mode, and comparison cell pairs color with a text label, symbol, or geometric indicator.
- **Forced Colors Mode**: active tab underlines use `border-block-end` on a `::after` pseudo-element (never `box-shadow` or background fills alone), and focus rings use `outline` with `outline-offset: 3px`.
- **No dynamic lightness-only OKLCH shifts**: use predefined tokens or `color-mix(in oklab, ...)` so colors stay in gamut without hue drift.

## Typography

- **Display and UI prose**: `"Instrument Sans", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, system-ui, sans-serif`
- **Code, signals, and accessibility trees**: `"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace`
- **Scale**: fluid `clamp()` display headline (`clamp(2.25rem, 4.5vw, 3.75rem)`, weight `700`, tracking `-0.03em`, line-height `1.08`); section headings `clamp(1.65rem, 2.8vw, 2.35rem)` (weight `650`, tracking `-0.02em`); body `1.0625rem` (line-height `1.62`, max measure `68ch` in `.prose` blocks).
- **Hierarchy**: single `<h1>` per page (skip-link target with `tabindex="-1"`), strictly sequential `<h2>` → `<h3>` headings, never `<div>` styled as headings.

## Layout, grid, and CSS budget

- **Rhythm**: 8px spacing unit (`--space-1: 0.5rem` through `--space-10: 5rem`), 1240px max container width, 24px gutters.
- **Prose convention**: editorial and documentation copy uses `.prose`-prefixed classes (`prose-section`, `prose-lead`, `prose-card`, `prose-grid`, `prose-callout`).
- **Budget**: total authored CSS must stay well below 1,800 LOC (target ≤ 900 LOC across `tokens.css`, `base.css`, `components.css`, and `prose.css`). No Tailwind or utility frameworks.
- **Asset versioning (`[WEBSITE-ASSET-VERSIONS]`)**: every site-local CSS and JS URL carries a build-generated `?v=<12-char-sha256>` content hash.
- **Touch targets**: all interactive controls enforce `min-block-size: 2.5rem` (`40px`, expanding to `44px` on `@media (pointer: coarse)`).

## Signature components

1. **Dual-Engine Hero & Mode Toggle**: shows the same Osprey Native app rendered via **Native Platform Controls** (SwiftUI / WinUI / Android) and **FerroUI GPU Canvas** (GPUI) side by side, making "not either/or" immediately tangible.
2. **Accessible Tabbed Switchers (`focusgroup="tablist nomemory"`)**: used for the **Syntax Flavor Switcher** (Default `.osp`, ML `.ospml`, Python flavor, TypeScript flavor, plus Dart & Kotlin roadmap) and the **Rendering Mode Inspector**. Uses `aria-selected`, `focusgroupstart`, `role="tabpanel"`, `hidden="until-found"` with `beforematch` sync, border-drawn `::after` underline, and an inner wrapper inside each panel so hidden panels never render empty padding strips.
3. **Interactive Accessibility Tree & `@osprey/uitest` Explorer**: lets visitors inspect a live UI machine's `Accessible` nodes (`Pressable`, `Slidable`, `Popup`, `MenuItem`), click a control label, and see the exact headless `@osprey/uitest` assertion and pointer event it triggers.
4. **Responsive Comparison Matrix**: semantic `<table>` with `<caption>`, `<th scope="col">`, `<th scope="row">`, sticky headers, keyboard-scrollable `tabindex="0" role="region" aria-labelledby`, and a `@container (width < 640px)` stacked card layout with CSS variable label injection.
5. **Verified Platform Gallery**: real captures from Windows (WinUI), macOS (SwiftUI & FerroUI GPUI), iOS (SwiftUI), Android (native views), and Web (WebAssembly), each in `<figure>`/`<figcaption>` with explicit `width`/`height` to prevent CLS.

## Motion and resilience

- All transitions respect `@media (prefers-reduced-motion: reduce)` by disabling transforms and transitions immediately.
- Content, navigation, code samples, and comparison tables remain readable and usable with JavaScript disabled.
- Every interactive control has a visible `:focus-visible` ring (`3px solid` with `3px` offset) and zero horizontal overflow at 375px mobile and 1440px desktop.
