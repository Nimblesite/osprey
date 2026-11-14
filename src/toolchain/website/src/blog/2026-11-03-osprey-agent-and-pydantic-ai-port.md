---
layout: page.njk
title: "Osprey Agent: a Coding Agent on the pydantic-ai Port"
excerpt: "The pydantic-ai agent engine is ported to Osprey's Python flavor, and a desktop coding agent runs on it with real file and command tools."
description: "Osprey ports the pydantic-ai 2.46.0 agent engine to its Python flavor and builds Osprey Agent, a desktop coding agent, on that port and Osprey Native."
date: 2026-11-03
tags: ["blog", "python-flavor", "pydantic-ai", "agents", "osprey-native"]
author: "Christian Findlay"
readingTime: 3
image: /assets/images/blog/osprey-agent-tools.png
imageWebp: /assets/images/blog/osprey-agent-tools.webp
imageAlt: "A glass osprey connects glowing file, code and terminal panels above a desktop."
---

Osprey now runs a tool-calling coding agent, and the engine behind it is a
Python-flavor port of [pydantic-ai](https://github.com/pydantic/pydantic-ai)
2.46.0. Neither needs a CPython interpreter: the port is Osprey source compiled
to native code, and the agent is a desktop application on Osprey Native.

## The port

[`@osprey/pydantic-ai`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/pydantic_ai)
reimplements enough of the upstream library to run a tool-calling agent against
any OpenAI-compatible chat API: the message and event shapes, the Chat
Completions request and response mapping, `TestModel`'s schema rules, the
tool-calling `Agent` loop, and the provider profiles. Osprey has no exceptions,
so a request or a run answers an `Outcome` value instead of throwing.

The port keeps upstream's names and shapes so an Osprey program reads like the
CPython one, but it is a reimplementation, not a copy, and the upstream Python
package is not bundled. The original work belongs to Pydantic Services Inc. and
its contributors, and this port is not endorsed by them; the licence and author
acknowledgement live in the package's
[NOTICE.md](https://github.com/Nimblesite/osprey/blob/main/src/osprey/packages/pydantic_ai/NOTICE.md).

## The agent

[Osprey Agent](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/agent) is
a coding agent as a desktop application: give it a project folder and a task,
and it reads, edits and runs commands through tools while showing its work as it
goes. Its interface is drawn by [Osprey Native](/docs/osprey-native/), and a
model request and a tool are capabilities, so the window stays live while the
agent works and two conversations can run at once. The turn logic is
`Agent.run_sync` taken apart step by step, the way CPython's `agent.iter` does
it.

The tools run on your machine with your permissions and no sandbox yet, so point
it only at a project you are happy for it to change. Without a key it asks for
one and keeps it out of the conversation file.

Read the full provenance and thanks on the [announcements and credits](/credits/)
page.
