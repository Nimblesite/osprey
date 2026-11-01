# Blog demand research · 1 November 2026

Google Trends (pytrends, worldwide, past 12 months, `today 12-m`) for the features added since the 27 October GPUI post: UI machines and the casting player, `@osprey/chromecast`, `@osprey/uitest` and type disciplines. Each post answers a question people already ask; the posts themselves never mention this research.

A fifth batch (rust gui, egui, tauri, gpui tutorial, iced; mypy, pyright, ty, python type hints) was refused twice with HTTP 429 and is not used. Interest is relative within one batch of five terms (0–100), so compare terms only inside a batch. Related queries are Google's "top" and "rising" lists.

## What people ask

| Batch | Term | Mean interest | Top and rising related queries |
| --- | --- | --- | --- |
| Casting | cast to chromecast | 21.4 | how to cast to chromecast (rising) |
| Casting | chromecast from pc | 0.2 | how to chromecast from pc, cast to chromecast from pc, chromecast from pc to tv |
| Casting | chromecast from mac | 1.2 | how to chromecast from mac, chromecast from mac to tv, stream to chromecast from mac |
| Casting | chromecast audio | 12.9 | chromecast audio app, chromecast audio alternatives (rising +140%) |
| Casting | chromecast api / python / google cast sdk | 0.3–1.2 | none |
| Typing | static typing | 0.3 | static typing python, what is static typing, static typing vs dynamic typing |
| Typing | type checker | 1.7 | ty type checker (rising +170%) |
| Typing | dependent types / refinement types | 1.7 / 0.0 | none |
| Agents | playwright mcp | 51.9 | playwright mcp server, claude mcp playwright, browser mcp |
| Agents | computer use agent | 36.9 | openai computer use, claude computer use, claude cowork (rising) |
| Agents | accessibility tree | 18.3 | — |
| Testing | ui testing | 3.0 | what is ui testing, ui testing tools, ui testing framework, automated ui testing |
| Testing | accessibility testing | 2.4 | automated accessibility testing, accessibility testing tools |
| UI | ui framework | 52.0 | best ui framework, python ui framework, rust ui framework (rising) |
| UI | gpui | 11.3 | rust gpui, what is gpui, gpui component, egui |
| UI | slint | 28.5 | slint ui, egui, tauri (rising) |
| Language | best programming language | 7.9 | best programming language for ai, best programming language to learn |
| Language | functional programming language | 0.8 | what is functional programming language, best functional programming language (rising) |

## Posts this decides

| Post | Question it answers | Feature it shows |
| --- | --- | --- |
| How to Chromecast from a Mac to a TV | how to chromecast from mac to tv; chromecast audio app | The casting player and its visualiser |
| Static typing for Python-style code, with no type hints | static typing python; what is static typing; static vs dynamic | `@osprey/chromecast`, a Python-flavor port checked at compile time |
| An accessibility tree that AI agents and UI tests can drive | computer use agent; accessibility tree; automated ui testing | `Templates::semantics`, `Templates::act`, `@osprey/uitest` |
| A Rust UI framework without writing Rust | rust ui framework; what is gpui; best ui framework | UI machines on the GPUI host |

Type disciplines get no post: "dependent types", "refinement types" and "type checker" in its language sense show almost no interest. The Chromecast developer terms (API, SDK, Python) show none either, so the Chromecast package is shown through the static-typing question and the user-facing casting question instead.
