---
layout: page
title: Proofs and mathematics in Osprey
description: Twelve short films introduce dependent types, the Proof Workbench, checked Lean exports and the Mathlib port, with separate paths for programmers and mathematicians.
permalink: /docs/proofs/
---

A proof states a precise claim and supplies evidence a checker can verify. Osprey has a proof kernel written in Osprey, a Workbench for building small arguments, and an ongoing library port that already checks real exported mathematical proofs.

## Choose your starting point

**For programmers:** start with dependent types, then use the Workbench. No calculus or prior proof-assistant experience is assumed. **For mathematicians:** choose the mathematics versions for terms, goals, assumptions and the import boundary. Both paths use real Osprey code and applications.

Each film runs about two to three minutes, with English captions, seekable chapters and a full transcript. Watch in this order, or choose a topic below.

| Topic | Programmer version | Mathematics version |
| --- | --- | --- |
{% for id, topic in proofTopics %}| {{ loop.index }}. [{{ topic.title }}](#{{ id }}) | [Watch](/docs/proofs/videos/{{ id }}/) | [Watch](/docs/proofs/videos/{{ id }}-math/) |
{% endfor %}

## Try the tools

- [Proof Workbench guide](/docs/proof-workbench/): open the app, follow the first proof, inspect goals and use guided lessons.
- [Mathlib port: current evidence](/docs/mathlib-port/): reproduce the checked imports and understand which claims remain open.
- [Dependent types and proofs specification](/spec/0049-dependenttypesandproofs/): the full intended contract, distinguished from delivered results.

{% for id, topic in proofTopics %}
<h2 id="{{ id }}">{{ topic.title }}</h2>

{{ topic.intro }}

[For programmers](/docs/proofs/videos/{{ id }}/) · [For mathematicians](/docs/proofs/videos/{{ id }}-math/)

{{ topic.detail }}
{% endfor %}

The flow demonstration supplies numerical evidence; it does not claim a formal Navier-Stokes proof. The imported corpus is a working step toward Mathlib, not the whole library. Osprey is actively evolving.
