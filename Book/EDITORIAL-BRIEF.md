# Editorial brief

## Positioning

*The Osprey Book* is the bridge between “I can copy a code sample” and “I can design a small, honest program.” It teaches programming through Osprey's practical functional core: values, functions, inferred types, pattern matching, explicit failure, effects, and isolated concurrency.

The book is not a compressed language specification. It is a guided build in which every new idea solves a problem the reader has already met.

## Reader

The primary reader is a young or early-career developer, roughly beginner to intermediate. They may have tried a school course, a scripting language, a game engine, or a coding agent, but the book does not assume they know compiler theory or functional-programming vocabulary.

The reader can create a text file and use a browser. A local terminal is introduced as a useful tool, not an entrance exam. Installation appears beside the no-install Playground path so toolchain setup never blocks the first success.

More experienced readers should still find a direct account of Osprey's type inference, explicit failure, effects, fibers, memory modes, and flavor boundary.

## Promise and tone

- Pragmatic, friendly, and technically exact
- Short paragraphs with one clear move
- Concrete code before abstraction
- Never childish, even when explaining a first principle
- No hype, fake rivalry, or initiation rituals
- Compiler errors are guidance, not proof that the reader is “bad at programming”
- Define the everyday idea first, then offer the precise term
- Prefer one evolving program over unrelated toy fragments
- Keep limitations beside the feature they qualify

The prose can be energetic. It must never sound breathless. “You made the computer do something” is better than “unlock revolutionary performance.”

## Functional-programming signal

The book gives functional programmers quiet proof that Osprey contains the real ideas: immutable bindings, expressions, Hindley–Milner inference, algebraic data types, exhaustive pattern matching, persistent collections, higher-order functions, and first-class effects.

Those names appear after their behavior is useful. A beginner first learns that a value does not change under their feet; an experienced reader can recognise immutability. A beginner sees every possible state written down; an FP reader can recognise a sum type. No chapter turns that recognition into a lecture aimed past the primary reader.

## Flavor policy

Default flavor is the book's teaching surface. Every core lesson and complete running example appears in `.osp` first.

ML flavor is an optional alternate surface introduced after the reader already understands the shared idea. It is never called a separate language, an advanced mode, or a choice that must be made up front. The book may show a compact ML twin in a “Same flight, different feathers” aside when the comparison reduces confusion.

The language architecture is open to more flavors. Avoid claims that Osprey will always have exactly two. Say “the currently available Default and ML flavors” when the current count matters.

A coding agent can translate surface syntax quickly, which makes experimentation approachable. The book still requires the reader to run `--check` and the relevant tests after translation. Agent assistance lowers typing cost; it does not replace evidence.

## Teaching pattern

Every chapter follows the same learning loop:

1. **Make something happen.** Start from an outcome the reader can see.
2. **Read the code.** Name only the syntax needed for that outcome.
3. **Change one thing.** Invite a safe prediction before the reader runs it.
4. **Meet the idea.** Explain the general principle in plain language.
5. **Let the compiler help.** Make one useful mistake and read the location and expectation.
6. **Build the Flight Log.** Add one bounded capability to the running project.
7. **Take the agent handoff.** Provide a paste-ready prompt with a verification command.
8. **Check the result.** Run the program or tests and state what is now known.

No chapter introduces more than four conceptual families. Code blocks should fit a phone or small e-reader without horizontal scrolling.

## Running project: Flight Log

The reader grows a small personal project that records things they want to learn, marks progress, explains failures, and eventually performs outside work. It begins as a printed launch message, then gains typed states, lists, safe parsing, effects, tests, persistence, and concurrent tasks.

The project is intentionally ordinary. It provides enough domain to make types and effects meaningful without requiring a framework, database, or prior application architecture.

## Chapter limits

- 2,200–3,600 words
- Five to eight core sections
- Four to nine short code or command blocks
- Two to four purposeful visuals
- One Flight Log checkpoint
- One compiler-feedback exercise
- One paste-ready agent handoff
- Five to seven closing takeaways

## Accuracy gates

- Every example is checked with the pinned Osprey compiler.
- Behavior claims cite a governing specification and an executable test where practical.
- Installation commands come from the maintained installation guide.
- A generated illustration never contains product output, syntax, diagnostics, or labels.
- Future work is visibly labelled as future work.
- Native, WebAssembly, effect-resumption, module, package, GPU, and C-FFI limits remain beside the relevant claim.

## Explicitly out of scope

- A compiler implementation textbook
- Category theory as a prerequisite
- A complete standard-library reference
- A promise that alpha software will never change
- Treating ML syntax as mandatory for “real” functional programming
- Pretending a coding agent makes checking and testing optional
- Teaching roadmap-only modules, packages, hardware GPU execution, or strict static memory as shipped features

