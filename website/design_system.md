# Osprey design system

Osprey looks like logic in flight: a midnight canvas, luminous cyan, precise type, and room to think. This applies to the website, diagrams, video, captions, and posters. Start every brief with [the messaging guide](../docs/messaging.md): show a need, choose a handler, show the result.

## Source of truth

[Design palette](../docs/designs/design.md) defines the brand; [CSS tokens](src/css/variables.css) supply its working values. Use those tokens directly. Video pages import the same CSS and the Osprey code grammar rather than inventing another palette or copying code. The TikTok repository supplies rendering, narration, and music; its social-reel styling is not Osprey's website styling.

## Color and depth

| Role | Token | Value | Use |
| --- | --- | --- | --- |
| Canvas | `--surface` | `#0c1325` | Full frame and page |
| Inset | `--surface-lowest` | `#070d1f` | Code and quiet backgrounds |
| Raised | `--surface-container` | `#191f32` | Cards and panels |
| Headline | `--on-surface` | `#dce1fb` | Primary text |
| Supporting text | `--on-surface-variant` | `#bdc8cd` | Descriptions and labels |
| Accent | `--primary-container` | `#77d7f4` | One focal word, active handler, progress |
| Link | `--primary` | `#bdeeff` | Actions and readable emphasis |
| Boundary | `--outline-variant` | `#3e484c` | Fine borders and dividers |
| Error | `--error` | `#ffb4ab` | Actual failure states |

Use tonal layers and a restrained cyan glow for depth. Keep warm accents and syntax colors semantic. No rainbow stripes, cream backgrounds, comic shadows, oversized stickers, or bouncing typography.

## Type and composition

Use Geist for prose and headlines, JetBrains Mono for code, labels, and output. Headlines use weight 500–600, tight tracking, and short lines; body copy uses weight 400 and comfortable leading. Avoid all-caps paragraphs. Code is source, not decoration: show enough context to understand it and keep it legible at the delivered size.

Use an 8px spacing rhythm, a 1200px website container, and 24px gutters. Give each section one headline and one idea. Cards use fine outlines and the existing 10–14px radii. Buttons use the site's primary and secondary styles; visible focus uses cyan with an offset. At small widths, stack the content without shrinking the type to fit a desktop composition.

## Motion

The bird starts automatically, muted and looping. Pause it when it leaves the viewport or the tab is hidden; keep a pause control and preserve the visitor's choice. Reduced-motion and data-saving preferences show the poster until the visitor opts in.

Use 150–250ms feedback for controls, a brief 8–12px entrance for a headline, and a slow, low-amplitude drift for atmospheric light. Reveal a section once as it enters view. Animate opacity and transforms; avoid perpetual page-wide repainting. Reduced motion shows the final state immediately. Content and navigation must remain usable without JavaScript.

## Video and sound

A homepage film is a short demonstration, not a feature inventory. Open with effects. Show real source, the handler choice, and its output; end with one next step and a concrete maturity qualification. Use the same bird, navy surfaces, cyan focus, fonts, syntax colors, and thin code-panel outlines as the page. A change of handler can become a visual transition: the code stays anchored while the implementation and result change.

Compose landscape films at 1920×1080 with at least 80px safe margins. Keep each headline to two short lines. Show one code idea at a time; captions sit in their own lower safe area. Produce H.264/AAC with fast start, a deliberate poster, English WebVTT captions, and a lightweight web encode.

Narration explains what the viewer sees in concrete language. Use the TikTok factory's licensed local voice and original upbeat score, with music ducked beneath speech. Sound starts only when the visitor plays the film. Decorative motion stays silent.

## Review

Read [messaging](../docs/messaging.md) before writing a frame. Check every code excerpt against its source and every claim against demonstrated behavior. Inspect the poster and representative frames; watch the encoded film with sound. Check the homepage at desktop and phone widths, automatic bird playback, pause/resume, reduced motion, captions, keyboard focus, and overflow. Build and run the website tests before handoff.
