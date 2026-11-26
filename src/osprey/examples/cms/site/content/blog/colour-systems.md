---
title: Colour systems that survive dark mode
date: 2026-10-14
topic: Design
summary: Name roles, not colours. A palette of tokens with a light and a dark value is the cheapest dark mode you will ever ship.
cover: /media/cover-dusk.svg
---

Every colour in an interface answers a question: *is this the page, a surface on it, or something that needs attention?* Name the answer, not the hex value.

## Roles over swatches

A role such as `panel` or `inkMuted` has two values, one per appearance. Components only ever name roles, so switching appearance is a table lookup rather than a redesign.

- Neutrals carry the structure.
- One accent carries the action.
- Status hues are rare, which is why they work.

> If everything is highlighted, nothing is.
