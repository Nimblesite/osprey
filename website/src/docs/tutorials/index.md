---
layout: page
title: "Osprey video tutorials"
description: Video tutorials for Osprey applications and tools, including VS Code package management, profiling, and native app interop.
permalink: /docs/tutorials/
tags: [tutorials, video]
---

Watch real Osprey applications and tools. Each video includes captions, chapters and a transcript; the accompanying guide explains how to use the feature.

## Proofs and mathematics

[Choose a proof learning path](/docs/proofs/): six topics, each with a programmer introduction and a mathematics version. Start with dependent types, then explore the Workbench, the running Mathlib port, deliberate proof rejection and numerical fluid evidence.

The series follows the [Lean oracle relationship](/docs/proofs/#lean-as-an-oracle): real reference proofs, Osprey's own checker, and explicit milestones toward independent operation.

## Applications and tools

{%- for item in collections.all | sort(false, false, "data.title") %}{% if item.data.video and not item.data.film and item.url.startsWith("/docs/") %}{% set v = item.data.video | video %}

## {{ v.name }}

{% videoCard item.data.video, item.url %}

{{ v.description }}
{%- endif %}{% endfor %}

## More to read

The [documentation home](/docs/) lists the written guides, including [web apps](/docs/web-apps/), [iOS and Android apps](/docs/mobile-apps/) and [algebraic effects](/docs/effects/).
