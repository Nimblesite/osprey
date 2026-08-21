# Chapter 2 — Give values useful names

Chapter 1 ended with two values called `name` and `project`. Those names made a one-line program easier to read, but they can do more than shorten a string. A good name records why a value exists.

In this chapter, the Flight Log gains a small plan: who owns it, what they want to learn, whether the plan is active, how many sessions they expect, and how long each session should be. The program will still print one line. The improvement is that every part of that line will come from a stable, meaningful value.

```osprey
fn main() = {
    let owner = "Mika"
    let goal = "Build a small Osprey CLI"
    let isActive = true
    let sessionCount = 3
    let minutesPerSession = 25
    print("${owner} | ${goal} | ${isActive} | ${sessionCount} | ${minutesPerSession}")
}
```

This is already a complete program. It introduces no new project machinery. It takes the single Flight Log line from Chapter 1 and gives each fact a place.

## Reader outcome

By the end of this chapter, you should be able to:

- choose names from the problem rather than from a value's storage type;
- use strings, booleans, and integers in one small program;
- pass named values into a function and interpolate them into text;
- explain why an immutable binding remains dependable;
- read a simple expression as inputs flowing to an output;
- recognise that checked integer arithmetic produces a visible `Result`; and
- let the compiler stop an attempted reassignment.

## Name the fact, not its container

Consider these bindings:

```osprey
let text = "Build a small Osprey CLI"
let flag = true
let number = 3

// The problem already has better words:
let goal = "Build a small Osprey CLI"
let isActive = true
let sessionCount = 3
```

The first three names are not false, but they are nearly useless. `text` tells you what the value is made of, not what it means. `flag` says only that the value switches something. `number` leaves every important question unanswered.

The final three bindings use words from the problem instead.

Now a reader can understand the values without searching for every later use. The compiler can already work out that `goal` is a string, `isActive` is a boolean, and `sessionCount` is an integer. Repeating those type names in the identifiers would help the compiler less and the reader barely at all.

Prefer `goal` to `goalString`, `isActive` to `activeBool`, and `sessionCount` to `sessionCountInt`. Add a type word only when it distinguishes two ideas people genuinely discuss that way, such as a `userId` and a `userName`.

A useful naming test is to read the binding aloud. “The goal is Build a small Osprey CLI” sounds like a fact from the application. “The text is Build a small Osprey CLI” sounds like a description of memory.

Names do not need to be long. They need to remove the next question.

Names also have a scope: the region of source in which they can be used. A binding inside `main` belongs to that invocation of `main`; a parameter belongs to its function body. Keeping names local reduces the amount of program a reader must hold in mind. `goal` does not claim to be the one goal for every future application. It identifies the goal for the small block that owns it.

This gives you two checks for a candidate name. First ask, “What fact from the problem does this value represent?” Then ask, “Is that fact precise within this scope?” `status` may be adequate in a five-line function with only one status, but `isActive` is clearer when planned, paused, and completed states will soon coexist. Good naming is not a contest to produce the longest identifier. It is choosing the shortest name that stays unambiguous where it is read.

## Values carry different kinds of information

The new plan uses three everyday kinds of value:

- A **string** holds text, such as `"Mika"` or `"Build a small Osprey CLI"`.
- A **boolean** is either `true` or `false`. `isActive` uses it to answer one yes-or-no question.
- An **integer** holds a whole number, such as `3` sessions or `25` minutes.

You do not need annotations to create these values. Their literal forms provide enough information: the quotes around `"Mika"` identify a string, `true` identifies a boolean, and the whole-number literal `3` identifies an integer. Osprey records those facts and checks later uses against them.

This is why names and types have different jobs. A type tells the compiler and reader what operations make sense. A name tells the reader why this particular value matters. Many unrelated facts can be strings; only one of these strings is the goal.

### Try it: predict a safe change

Change `isActive` from `true` to `false`. Before running the program, predict which part of the output changes and which parts remain identical.

Then change `sessionCount` from `3` to `4` and predict again.

These edits replace one input value. They do not change the program's structure, the meaning of the other bindings, or the types involved. A precise prediction is possible because the values have stable names.

## Build a line from named values

Move the formatting into a function so `main` owns the facts and `summary` owns their presentation:

```osprey
fn summary(owner, goal, isActive, sessionCount) =
    "${owner} | goal: ${goal} | active: ${isActive} | sessions: ${sessionCount}"

fn main() = {
    let owner = "Mika"
    let goal = "Build a small Osprey CLI"
    let isActive = true
    let sessionCount = 3
    summary(owner, goal, isActive, sessionCount) |> print
}
```

The parameters use the same problem words as the bindings. That is intentional. At the call site, `owner` is a binding in `main`. Inside `summary`, `owner` is a parameter receiving the supplied value. Their scopes differ, but the shared name keeps the idea continuous.

String interpolation places each expression between `${` and `}`. Osprey turns the value into readable text as part of building the result. The boolean becomes `true`; the integer becomes `3`.

`summary` does not print. It produces a string. `main` pipes that string into `print`. Keeping those two jobs separate makes the function easy to reuse later when the Flight Log writes to a file or sends a response instead of using the terminal.

![Five immutable Flight Log values flow into a summary function and produce one output line.](assets/diagrams/02-value-graph.png)

*Figure 2.1 — The names reveal a value graph: stable facts enter one function, which produces one new value.*

The diagram is not a timeline. `owner` does not turn into `goal`, and `goal` does not turn into `isActive`. The values all remain available. `summary` reads them and creates another value from them.

## Derive values instead of changing them

Add one more input, `minutesPerSession`, and calculate the planned time:

```osprey
fn plannedMinutes(sessionCount, minutesPerSession) =
    sessionCount * minutesPerSession

fn summary(owner, goal, isActive, sessionCount, minutesPerSession) =
    "${owner} | goal: ${goal} | active: ${isActive} | sessions: ${sessionCount} | minutes: ${plannedMinutes(sessionCount, minutesPerSession)}"
```

`plannedMinutes` receives two values and produces a result. It does not update either input. After the call, `sessionCount` still means `3` and `minutesPerSession` still means `25`.

That gives you a useful way to read an expression:

> Put these input values into this operation and name or return the value that comes out.

An expression is not merely a shorter statement. It has a value. A function whose body is one expression produces that expression's value without a separate `return` line.

This style scales because the data flow remains visible. If the plan changes to four sessions, you replace the `sessionCount` input. You do not hunt for a counter that several steps may have changed.

It also keeps presentation separate from calculation. `plannedMinutes` does not know that its result will appear in a terminal line. `summary` does not know how multiplication is checked. Each function has one reason to change. A different report can reuse the calculation, and a different time policy can replace the calculation without rewriting every piece of text.

Small expression-shaped functions are not a demand to compress a whole application into one line. They are a way to keep each transformation narrow enough to name. When a function begins to answer several unrelated questions, split the questions and let the results flow into the next expression.

### Under the wing: immutability and repeatable meaning

An immutable binding keeps the same meaning within its scope. Given the same `sessionCount` and `minutesPerSession`, `plannedMinutes` produces the same result. It does not inspect a hidden global counter or modify either argument.

Functional programmers call this kind of repeatable relationship **referential transparency**: an expression can be understood through its value without a hidden change elsewhere. You do not need the term to use the benefit. Stable inputs make local reasoning possible.

Immutability does not mean an application can never represent change. Later the Flight Log will create a new state value that differs from an earlier one. The key is that the earlier value does not change under your feet.

## Arithmetic keeps failure visible

You might expect `3 * 25` to have the integer value `75`. In Osprey, integer arithmetic is checked for overflow. The multiplication therefore produces a `Result` with one of two shapes:

- `Success(75)` when the answer fits in an integer;
- `Error(integer overflow)` when it does not.

![Two integer inputs flow through checked multiplication to either a successful value or an overflow error.](assets/diagrams/02-expression-flow.png)

*Figure 2.2 — Checked arithmetic refuses to replace an impossible answer with a plausible wrapped number.*

The full summary prints the successful result as `Mika | goal: Build a small Osprey CLI | active: true | sessions: 3 | minutes: Success(75)`. That wrapper is evidence, not clutter added by the example.

For now, leave the result visible. Chapter 7 will open both branches with `match` and decide what the Flight Log should do on failure. Introducing the shape now prevents a dangerous assumption: integer operations do not silently wrap around to a different number.

Floating-point arithmetic follows different machine rules and does not use this checked integer `Result` contract. That distinction matters when choosing a numeric model. A count of sessions is an integer, so the checked behavior is appropriate.

### Try it: follow the expression

Change `sessionCount` to `4` while leaving `minutesPerSession` at `25`.

Predict the complete final field before running. The multiplication has new inputs, so `plannedMinutes` should produce `Success(100)`. The owner, goal, and active fields should remain unchanged.

Run the program and compare the exact output with your prediction. If something else changed, inspect the values passed to `summary` before changing the function.

## Let the compiler protect a binding

An immutable binding is not a box waiting to be updated. Try to treat it like one:

```osprey
fn main() = {
    let sessionCount = 3
    sessionCount = 4
    print(sessionCount)
}
```

Run `osprey flight-log.osp --check`. The check fails at the attempted assignment because `sessionCount` was bound with `let` and cannot be reassigned.

Read the real diagnostic rather than memorising wording that may change while Osprey is in alpha. It should identify the location, the immutable name, and the forbidden assignment.

The repair is not to search for a spelling that forces mutation. Decide which fact the program means:

- If the plan has four sessions, bind `sessionCount` to `4` in the first place.
- If you need both plans, give each value a name such as `originalSessionCount` and `revisedSessionCount`.
- If the application later moves between states, create a new state value from the old one.

Each repair makes the data model more truthful. A hidden reassignment would make earlier reasoning unreliable.

Compiler feedback is most useful when you preserve the original question. Here the question is not “How can I make this assignment compile?” It is “Which value should the later expression read?” The source already has enough structure to answer: edit the original plan, name both plans, or model a transition. Treating the diagnostic as design feedback prevents a quick syntax repair from creating a confusing data story.

## Flight Log checkpoint

Replace the Chapter 1 source with the complete Chapter 2 program in `examples/chapter-02/flight-log.osp`:

```osprey
fn plannedMinutes(sessionCount, minutesPerSession) =
    sessionCount * minutesPerSession

fn summary(owner, goal, isActive, sessionCount, minutesPerSession) =
    "${owner} | goal: ${goal} | active: ${isActive} | sessions: ${sessionCount} | minutes: ${plannedMinutes(sessionCount, minutesPerSession)}"

fn main() = {
    let owner = "Mika"
    let goal = "Build a small Osprey CLI"
    let isActive = true
    let sessionCount = 3
    let minutesPerSession = 25
    summary(owner, goal, isActive, sessionCount, minutesPerSession) |> print
}
```

Personalise `owner` and `goal`. Choose a realistic session count and session length. Predict the planned-minutes field, then check and run:

```sh
osprey flight-log.osp --check
osprey flight-log.osp --run
```

The landing condition is one readable summary whose facts come from meaningful immutable bindings. Its final field must be a visible `Success` containing your calculated minutes.

## Agent handoff

Paste this into a coding agent if you want help applying the lesson to your file:

```text
Update flight-log.osp in Osprey Default flavor.

Keep one immutable binding for the owner, goal, active status, session count,
and minutes per session. Name each value for its role in the Flight Log, not
for its type. Keep plannedMinutes pure and let checked integer multiplication
return its Result visibly. Build one summary string and use only one print path.
Do not add inferable type annotations or mutable bindings.

Run:
osprey flight-log.osp --check
osprey flight-log.osp --run

Report the exact output and explain which source value produced each field.
```

Review the names before accepting the edit. An agent can produce valid code with names that are technically correct but vague in your domain. Ask it to justify `data`, `value`, `flag`, or `number` if any of those appear.

## What changed

- Names such as `goal` and `sessionCount` recorded why values existed.
- Strings, booleans, and integers carried different kinds of information.
- Type inference kept those values checked without repetitive annotations.
- `summary` combined stable inputs into one new string.
- `plannedMinutes` expressed a calculation without changing either input.
- Checked integer multiplication returned `Success(75)` instead of risking silent overflow.
- The compiler rejected an attempted reassignment to an immutable binding.

Chapter 3 follows the types the compiler inferred through these same expressions. You will learn when a type annotation adds information, when it merely repeats the obvious, and how to read a mismatch without guessing.

## Authoritative sources

- Osprey [Syntax](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0003-Syntax.md) for immutable Default-flavor bindings, functions, blocks, and expressions.
- Osprey [Type System](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0004-TypeSystem.md) for inferred primitive types and function results.
- Osprey [String Interpolation](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0006-StringInterpolation.md) for embedded value rendering.
- Osprey [Error Handling](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0013-ErrorHandling.md) for checked integer arithmetic and visible overflow.
- The executable Chapter 2 Flight Log source and expected output in `examples/chapter-02/`.
