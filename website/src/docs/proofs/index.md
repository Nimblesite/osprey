---
layout: page
title: Proofs and mathematics in Osprey
description: Twelve short films introduce dependent types, the Proof Workbench, checked Lean exports and the Mathlib port, with separate paths for programmers and mathematicians.
permalink: /docs/proofs/
---

A proof states a precise claim and supplies evidence a checker can verify. Osprey has a proof kernel written in Osprey, a Workbench for building small arguments, and an ongoing library port that already checks real exported mathematical proofs.

## Lean as an oracle

Lean supplies reference statements, proof certificates and executed checks while Osprey develops its own proof technology. Osprey's kernel makes its own decisions; source authoring, automation and library reconstruction have separate completion milestones. The goal is independent operation with replaceable disciplines and room for further capabilities.

An oracle can contain implementation defects. Known unsound certificates must be rejected even if an affected Lean release accepts them. A corrected checker, changed axiom policy or forked theory needs its own identity and fresh validation of dependent proofs; previous Checked badges cannot simply carry over. This does not assert that Lean's underlying mathematical theory is inconsistent.

The [current evidence guide](/docs/mathlib-port/) distinguishes delivered results from remaining work. The [completion milestones](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0041-dependent-types-and-mathlib.md#completion-milestones-proof-completion) specify the source frontend, automation, full library and independent-operation gates.

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

### For programmers

{% videoPlayer id, true %}

### For mathematicians

{% videoPlayer id + "-math", true %}

{{ topic.detail }}
{% endfor %}

The flow demonstration supplies numerical evidence; it does not claim a formal Navier-Stokes proof. The imported corpus is a working step toward Mathlib, not the whole library. Osprey is actively evolving.
