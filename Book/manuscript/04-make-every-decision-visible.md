# Chapter 4 — Make every decision visible

Flight Log needs to say whether a lesson is planned or complete. A decision should produce the right words for each state, and the compiler should catch a forgotten state before you run the program.

Start with a smaller decision: turn a boolean into a useful label.

## Choose a value

```osprey
fn activityLabel(active) = match active {
    true => "ready to learn"
    false => "taking a break"
}
```

`match` examines the expression after the keyword. Each line inside the braces is an **arm**: a pattern on the left of `=>`, followed by the expression to evaluate when that pattern matches. Here the patterns are the two boolean values.

Calling `activityLabel(true)` returns `"ready to learn"`. Calling it with `false` returns `"taking a break"`. The function returns text; its caller decides whether to print it.

As with the functions in Chapter 3, the compiler works out the types. The boolean patterns require a boolean input. Both arms return strings, so the result is a string.

A match is an expression: it produces a value. You can bind that value with `let`, pass it to another function, or use it as a function's result. Its arms must agree on the result type. Changing one arm to return an integer while the other returns text creates a type mismatch.

**Try it.** Predict `activityLabel(false)` before running it. Then change only the false arm's text. The true result should stay the same.

## Cover the remaining values

A boolean has two possibilities. An integer has far more, so name the values that matter and give the rest one answer:

```osprey
fn sessionLabel(count) = match count {
    0 => "first flight"
    1 => "one session logged"
    _ => "several sessions logged"
}
```

The literal patterns `0` and `1` match those exact values. `_` is a **wildcard**: it matches anything still remaining and creates no binding. In this function, `2`, `3`, and `20` all receive the last label.

That is a policy you chose. The wildcard also accepts negative numbers. This function assumes the caller supplies a valid session count; a wildcard does not validate its input.

Put the wildcard last. If it came first, it would cover every value and leave the later arms unreachable. The compiler rejects arms after a catch-all.

The complete [decisions example](https://github.com/Nimblesite/osprey/blob/main/Book/examples/chapter-04/decisions.osp) calls both functions. From `Book`, run:

```sh
../target/release/osprey examples/chapter-04/decisions.osp --run
```

```text
ready to learn
taking a break
first flight
one session logged
several sessions logged
```

**Try it.** Change the final call from `sessionLabel(3)` to `sessionLabel(0)`. Predict which line changes and which arm supplies it.

## Name the states

Strings can label a state, but they also admit misspellings. Give Flight Log a type whose possible states are written down:

```osprey
type Status = Planned | Complete { note: string }

fn statusLabel(status) = match status {
    Planned => "planned"
    Complete { note } => "complete: ${note}"
}
```

`Status` is a **union**. The `|` separates its alternatives, called **variants**. A status is either `Planned` or `Complete`. `Planned` carries no extra data. `Complete` carries a string field named `note`.

Construct a completed status with `Complete { note: "handled both cases" }`. Its note belongs to that completed value; a planned value needs no note.

In the match, `Complete { note }` selects that variant and binds its stored field to the name `note`. The expression after `=>` can use it. That name belongs to this arm; it does not become a variable in the `Planned` arm or after the match. Unpacking a value this way is called **destructuring**.

![A Status follows either the Planned arm to planned text or the Complete arm, which binds its note and produces completion text.](assets/diagrams/04-status-paths.png)

*Figure 4.1 — Each status has a route to a string.*

Both variants appear explicitly. That makes the function **exhaustive**: every possible status has an answer. The compiler checks this for booleans and known unions. For an integer decision such as `sessionLabel`, include a catch-all yourself so every input has an answer; the compiler does not prove integer coverage.

## Let the compiler catch a forgotten state

Suppose you add a state for a lesson you have started:

```osprey
type Status = Planned | Learning | Complete { note: string }
```

Leave `statusLabel` unchanged and check the supplied mistake:

```sh
../target/release/osprey \
  examples/chapter-04/failscompilation/missing-case.osp --check
```

The compiler reports:

```text
examples/chapter-04/failscompilation/missing-case.osp: non-exhaustive match on `Status`: missing Learning
```

**Compiler says.** `Learning` is now a valid input, but the renderer has no answer for it. Add `Learning => "learning"` between the existing arms and check again. The decision now covers all three variants.

You could use a wildcard to cover the new state. For this renderer, an explicit arm is more useful: a future state deserves a deliberate label, and a missing-case error points you to that decision.

![Adding Learning to Status leaves a missing route until statusLabel receives a Learning arm; then checking succeeds.](assets/diagrams/04-missing-case.png)

*Figure 4.2 — A new variant creates a decision to finish.*

The [unreachable-arm example](https://github.com/Nimblesite/osprey/blob/main/Book/examples/chapter-04/failscompilation/unreachable-arm.osp) makes the other mistake: `_` appears before `false`. Check it, then move the specific arm before the wildcard. A catch-all covers the remaining cases only when earlier arms have had their turn.

## Flight Log checkpoint

Keep the two-state version for this checkpoint. Group a lesson's title and status in a record, then ask the status renderer for its part of the summary:

```osprey
type Lesson = { title: string, status: Status }

fn summary(lesson) =
    "${lesson.title} [${statusLabel(lesson.status)}]"
```

The complete [Flight Log source](https://github.com/Nimblesite/osprey/blob/main/Book/examples/chapter-04/flight-log.osp) constructs one planned lesson and one completed lesson. Running it prints:

```text
Read a match [planned]
Read a match [complete: handled both cases]
```

Check both lines against [the recorded output](https://github.com/Nimblesite/osprey/blob/main/Book/examples/chapter-04/flight-log.expectedoutput). Together they exercise both variants and show that the completed note survives construction, matching, and formatting.

Change the completed note and predict the second line. Then add `Learning` and its label to this checkpoint. Construct a learning lesson and print its summary too. You have changed the data model and finished the corresponding decision.

## Agent handoff

> Extend `Book/examples/chapter-04/flight-log.osp` with a `Learning` status. Give it an explicit `"learning"` arm and include a learning lesson in the printed transcript. Keep the planned and completed lines intact. From `Book`, run `../target/release/osprey examples/chapter-04/flight-log.osp --check` and `../target/release/osprey examples/chapter-04/flight-log.osp --run`. Compare the output with your prediction, then update the expected-output file. Show the diff and command results.

## Landing check

You can now:

- use a match's result as an ordinary value;
- choose exact values and put a wildcard last;
- declare a union with named alternatives;
- unpack a variant's payload inside its arm; and
- repair a missing case or an unreachable arm using compiler feedback.

Next, Flight Log will move from one lesson to a collection of lessons.
