---
layout: page
title: "Osprey video tutorials"
description: Video tutorials that teach Osprey through real code - effects for I/O, Python-flavor type inference, GPUI apps, Swift interop, UI testing by label and profiling.
permalink: /docs/tutorials/
tags: [tutorials, video]
---

Each tutorial teaches one part of Osprey through real code from the repository. It shows the files, runs them, and makes one change so you see its effect. Every video has captions, chapters and a transcript.

## Proofs and mathematics

[Choose a proof learning path](/docs/proofs/): six topics, each with a programmer introduction and a mathematics version. Start with dependent types, then explore the Workbench, the running Mathlib port, deliberate proof rejection and numerical fluid evidence.

The series follows the [Lean oracle relationship](/docs/proofs/#lean-as-an-oracle): real reference proofs, Osprey's own checker, and explicit milestones toward independent operation.

## Learn Osprey

{%- for item in collections.all | sort(false, false, "data.title") %}{% if item.data.video and not item.data.film and item.url.startsWith("/docs/") %}{% set v = item.data.video | video %}{% if v.kind != "intro" %}

## {{ v.name }}

{% videoCard item.data.video, item.url %}

{{ v.description }}
{%- endif %}{% endif %}{% endfor %}

## More to read

The [documentation home](/docs/) lists the written guides, including [web apps](/docs/web-apps/), [iOS and Android apps](/docs/mobile-apps/) and [algebraic effects](/docs/effects/).
