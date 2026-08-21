# The Osprey Book

*The Osprey Book* is a practical introduction to programming with Osprey for beginner-to-intermediate developers. It starts with a program you can run in minutes, then builds toward typed data, explicit failure, effects, concurrency, and native or WebAssembly delivery.

The current edition is a **structural scaffold with Chapters 1 and 2 complete**. It establishes the learning journey, teaching contract, source policy, production metadata, visual language, runnable examples for both completed chapters, and a working EPUB/HTML pipeline. Later chapters are mapped as editorial scaffolds rather than presented as finished prose.

## Reader promise

By the end of the finished book, a reader should be able to:

- read and write small Osprey programs in the Default flavor;
- let the compiler infer ordinary types without losing strong checks;
- model a problem with records, unions, and exhaustive pattern matching;
- transform collections with functions and pipelines;
- treat expected failure as data rather than a hidden exit;
- separate pure decisions from outside work with effects;
- test behavior and read compiler feedback without panic;
- use fibers for isolated concurrent work;
- compile a program for a native machine or WebAssembly; and
- recognise ML and future flavors as optional source surfaces over the same language.

## The teaching path

The book leads with the Default flavor (`.osp`). Its braces, `fn`, `let`, and parenthesised calls are familiar to readers arriving from JavaScript, TypeScript, C#, Java, Kotlin, Swift, Go, or Rust.

ML flavor appears later as an **optional alternative**, not a decision the reader must make before learning Osprey. A file can be translated between surfaces—often conveniently with a coding agent—then checked and tested like any other code change. The design remains open to more flavors in the future; the book teaches the shared language before touring alternative spellings.

## Project map

```text
Book/
├── book.json                 # canonical reading order and production targets
├── metadata.yaml             # publication metadata
├── OUTLINE.md                # detailed chapter architecture
├── EDITORIAL-BRIEF.md        # audience, voice, teaching pattern, and scope
├── SOURCE-POLICY.md          # authority, evidence, and accuracy rules
├── VISUAL-DESIGN-SYSTEM.md   # First Flight adaptation of Midnight Synthetic
├── GLOSSARY.md               # beginner-facing vocabulary authority
├── sources.json              # approved source ledger
├── evidence.json             # chapter claim-readiness ledger
├── figures.json              # planned and completed visual ledger
├── manuscript/               # front matter, chapters, and appendices
├── examples/chapter-*/       # runnable examples from completed chapters
├── assets/                   # cover, diagrams, illustration, and future captures
├── styles/                   # EPUB and standalone HTML styling
└── dist/                     # generated output; never hand-edited
```

## Production commands

```sh
make check          # validate manifests and parse every manuscript file
make check-examples # compile and run examples from completed chapters
make render-assets  # render deterministic SVG masters to publication PNGs
make epub           # build and validate the structural EPUB
make html           # build a standalone HTML reading copy
make release        # run every check and produce both formats
```

## Drafting rules

1. Treat `book.json` as the source of reading order and production targets.
2. Teach the Default flavor first; show ML only when comparison helps the reader.
3. Explain an everyday programming idea before introducing its specialist name.
4. Keep examples runnable against the pinned Osprey edition and store their output where the chapter can verify it.
5. Let code prove language behavior. Use generated illustration for mood, deterministic diagrams for explanation, and direct captures for product evidence.
6. Never promise a roadmap feature as shipped behavior.
7. Do not hide Osprey's alpha status, platform limits, or C safety boundary.
8. Run `make release` before publishing an edition artifact.

See [OUTLINE.md](OUTLINE.md) for the complete journey and [GLOSSARY.md](GLOSSARY.md) for the shared teaching vocabulary.
