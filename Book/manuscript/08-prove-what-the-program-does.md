# Chapter 8 — Prove what the program does

A compiler can tell you that a lesson title is text. It cannot tell you that the title still contains the learner's goal. Both `"Lesson: Read a match"` and `"Lesson: something else"` are valid strings. Only one is the answer we wanted.

The Flight Log now has decisions, explicit states, and results that can explain failure. This chapter turns its promises into checks you can run whenever you change it. We will test the words a learner sees, the progression of a lesson, and the limits on a session estimate.

Start with one small claim:

```osprey
fn lessonTitle(goal) = "Lesson: " + goal

test("title keeps the learner's goal", fn() =>
    expect(lessonTitle("Read a match"), "Lesson: Read a match")
)
```

This is the complete source of `examples/chapter-08/title.test.osp`. The suffix `.test.osp` lets Osprey's test runner discover it. It is still an ordinary Default-flavor program.

## Make one behavior claim executable

From the `Book` directory, check and run it:

```sh
../target/release/osprey examples/chapter-08/title.test.osp --check
../target/release/osprey examples/chapter-08/title.test.osp --run
```

With an installed compiler, use `osprey` in place of `../target/release/osprey`. The check confirms that the program fits together. The run executes the test and prints:

```text
ok 1 - title keeps the learner's goal
1..1
# tests=1 passed=1 failed=0 skipped=0
```

Read the source from the outside inward. `test` receives a name and a zero-argument function. The name describes the promise. `fn() => ...` supplies the work that the test runner will call while it is recording this case's result.

The body calls `expect(actual, expected)`. The first argument is what the program produced; the second is what we intended. `lessonTitle` computes the actual text. The literal string provides an independent expected answer.

That comparison is an **assertion**: an executable claim that a condition should hold. The named `test` provides a home for one or several related assertions. You can read its name without understanding the implementation and still know which promise failed.

The output follows a simple testing protocol called TAP. `ok 1` means the first named case passed. `1..1` reports that one case ran. The final line counts the cases. These are case counts, not counts of individual assertions; a case with three assertions still contributes one test.

A test also returns a process exit status. A successful run exits with `0`; a failed assertion makes the run exit with `1`. That allows a build command or an editor to notice failure without a person reading every line.

### Try it: keep the claim independent

Change the goal to `"Understand Result"` in both the function call and the expected string. Predict the exact title before running. The passing report stays the same because the test's name has not changed; the assertion now checks another example.

An answer such as `expect(lessonTitle(goal), lessonTitle(goal))` loses the useful comparison. If the function drops the goal, both arguments drop it together. The test can still pass. Write the expected result from the requirement, not by calling the same implementation a second time.

## Decide what a passing test establishes

Passing means the claims actually executed by this run held. It does not mean every possible use is correct. Our first test checks one goal, including its spaces and capitalization. It says nothing yet about an empty goal or a goal containing non-ASCII characters.

That limit is useful when choosing the next case. Ask what plausible mistake could remain. A function that accidentally removes spaces needs a phrase containing spaces. A function that always returns the same title needs a second, different title. Adding another copy of an unchanged assertion gives little new evidence.

![A type check asks whether values fit together, a named test checks a chosen behavior, and an output comparison checks the full visible transcript.](assets/diagrams/08-evidence-boundaries.png)

*Figure 8.1 — These checks answer different questions. A clean type check does not establish the wording of a summary, and matching output does not test inputs the program never received.*

There are three useful kinds of evidence in this book. Type checking rejects incompatible combinations before execution. Named tests compare selected behavior with an intended answer. A stored expected-output file checks a complete transcript, including line order and punctuation.

A **golden file** is that stored expected output. The book's `make check-examples` command compares each runnable example with its sibling `.expectedoutput` file. It also checks separate examples that must fail compilation. Changing a golden file is changing the expected behavior; read the difference before accepting it.

The current assertion helpers have a practical limit: they compare supported scalar values through their rendered forms. For example, integer `5` and string `"5"` render alike and can compare equal. Use compatible types in your assertions and leave type questions to the compiler. Lists and records are not direct `expect` operands; test their relevant contents or the output your program promises.

For our summary, exact text is part of the promise. For a `Lesson` record, the meaningful questions concern its title and status. We will avoid manufacturing a giant printed record merely to satisfy the assertion helper.

## Flight Log checkpoint: test the small decisions

Our checkpoint has a clear policy. A lesson starts `Planned`, moves to `Learning`, then becomes `Complete`. Advancing a completed lesson leaves it complete. A session estimate must be a whole number from 1 through 60 minutes, including both endpoints.

These are application decisions, so the tests should make them visible. A different project could choose a longer session or allow reopening lessons. The types alone cannot choose those policies for us.

Use the complete `examples/chapter-08/flight-log.test.osp`:

```osprey
type Status = Planned | Learning | Complete
type Lesson = { title: string, status: Status }

fn statusLabel(status) = match status {
    Planned => "planned"
    Learning => "learning"
    Complete => "complete"
}

fn advance(status) = match status {
    Planned => Learning
    Learning => Complete
    Complete => Complete
}

fn summary(lesson) =
    "${lesson.title} [${statusLabel(lesson.status)}]"

fn parseEstimate(text) = match parseInt(text) {
    Success { value } => match value >= 1 && value <= 60 {
        true => Success { value: value }
        false => Error { message: "estimate must be 1 to 60 minutes" }
    }
    Error { message } => Error { message: message }
}

fn accepts(text, expected) = match parseEstimate(text) {
    Success { value } => value == expected
    Error => false
}

fn rejects(text, expected) = match parseEstimate(text) {
    Success => false
    Error { message } => message == expected
}

test("each status has its own label", fn() => {
    check("planned", "planned", statusLabel(Planned))
    check("learning", "learning", statusLabel(Learning))
    check("complete", "complete", statusLabel(Complete))
})

test("advancing completes a lesson and stays complete", fn() =>
    checkAll("status transitions", [
        advance(Planned) == Learning,
        advance(Learning) == Complete,
        advance(Complete) == Complete
    ])
)

test("summary keeps the title and shows current status", fn() => {
    let lesson = Lesson { title: "Read a match", status: Learning }
    expect(summary(lesson), "Read a match [learning]")
})

test("estimates include both endpoints", fn() =>
    checkAll("accepted minutes", [
        accepts("1", 1), accepts("25", 25), accepts("60", 60)
    ])
)

test("estimates preserve the integer parser's message", fn() =>
    checkAll("invalid text", [
        rejects("soon", "parseInt: invalid integer"),
        rejects("25x", "parseInt: invalid integer")
    ])
)

test("estimates reject values outside the session range", fn() =>
    checkAll("out of range", [
        rejects("0", "estimate must be 1 to 60 minutes"),
        rejects("-1", "estimate must be 1 to 60 minutes"),
        rejects("61", "estimate must be 1 to 60 minutes")
    ])
)
```

Read the first half before reading the tests. `Status` lists the three possibilities. `Lesson` groups a title with one status. `statusLabel` gives every state a display string, and `advance` states what comes next in every case. Exhaustive matches make the cases visible.

`summary` accepts one lesson and returns one string. It does not print. This keeps the decision about wording separate from the decision about where to display it. A test can inspect the returned text directly.

`parseEstimate` makes two decisions in order. First, `parseInt` tries to read an integer. On success, the inner match checks the allowed range. A valid value is returned in `Success`; an out-of-range integer gets our application message. If parsing fails, the outer `Error` arm preserves the parser's message.

The `accepts` and `rejects` helpers belong to the tests. Both inspect the actual `Result` variant. `accepts` requires a successful value equal to the expected minutes. `rejects` requires an error with the expected message. A result with the wrong variant makes either check return `false`.

There is deliberately no default such as `parseEstimate(text) ?: 25` in those helpers. That would make invalid text look like an ordinary estimate of 25 minutes. Tests should observe the branch the program really took.

The application functions here are **pure**: their results depend on their inputs, and they do not perform outside work. No file, clock, network service, or hidden changeable setting is involved. Calling `summary` with the same lesson gives the same text. That makes mistakes straightforward to reproduce.

## Choose cases that catch believable mistakes

The first test uses `check(label, expected, actual)`. Notice its argument order: unlike `expect`, `check` puts the expected value before the actual value. The label makes a mismatch easier to locate when several assertions share a test.

The next test uses `checkAll(label, [conditions])`. Each element of this non-empty boolean list literal becomes an assertion. The three conditions enumerate the three transitions. Grouping them keeps one coherent promise together: advancing a lesson follows the stated progression.

Assertions are **soft**: a mismatch marks the case as failed, and execution continues. Other assertions in the case can report their mismatches too. This is helpful for related checks, but give unrelated behavior separate named cases so a failure report remains readable.

The estimate tests divide the input space into meaningful groups:

| Input | Expected result | Mistake it can expose |
|---|---|---|
| `"1"` | Success with 1 | Accidentally excluding the lower endpoint |
| `"25"` | Success with 25 | Replacing accepted input with a fixed number |
| `"60"` | Success with 60 | Accidentally excluding the upper endpoint |
| `"soon"`, `"25x"` | Parser error preserved | Accepting invalid text or dropping the reason |
| `"0"`, `"-1"` | Range error | Accepting zero or negative time |
| `"61"` | Range error | Accepting a session above the limit |

Boundary cases deserve attention because one character can change a rule: `value < 60` rejects 60, while `value <= 60` accepts it. An ordinary middle value like 25 cannot distinguish those implementations.

Run the file with `../target/release/osprey examples/chapter-08/flight-log.test.osp --run`. Its exact report is:

```text
ok 1 - each status has its own label
ok 2 - advancing completes a lesson and stays complete
ok 3 - summary keeps the title and shows current status
ok 4 - estimates include both endpoints
ok 5 - estimates preserve the integer parser's message
ok 6 - estimates reject values outside the session range
1..6
# tests=6 passed=6 failed=0 skipped=0
```

### Try it: test a useful property

In a copy, change only `Complete => Complete` to `Complete => Planned` inside `advance`. Predict which named case fails, then run it. Restore the original after reading the report.

The answer is “advancing completes a lesson and stays complete.” The status labels and summary still work. This is evidence that the transition test can detect a real violation, rather than merely execute the function. The last transition also demonstrates **idempotence** for completed lessons: applying this operation again leaves that state unchanged.

Do the same with `value <= 60`, changing it to `value < 60`. This time “estimates include both endpoints” must fail. Restore the code. These tiny temporary changes help you understand what each test protects.

## Read a failed assertion before changing anything

Return to the small title example. In a copy, remove `Lesson: ` from only the expected string. Leave the function unchanged. The program still passes `--check`: both strings have compatible types. Running it now produces:

```text
# expect failed: expected Read a match, got Lesson: Read a match
not ok 1 - title keeps the learner's goal
1..1
# tests=1 passed=0 failed=1 skipped=0
```

The program compiled and ran. An assertion then found a disagreement. That is a test failure, and the first line contains the useful evidence: expected text followed by actual text.

Do not immediately edit whichever side is easier. Read the promise again. If titles should include `Lesson: `, the changed expectation is wrong. If the product deliberately changes its title format, the function and its expectation need a coordinated update. The failure alone cannot decide which requirement you want.

![Write a behavior claim, check the program, run it, read any mismatch, repair the intended behavior or expectation, and rerun the suite.](assets/diagrams/08-test-feedback.png)

*Figure 8.2 — A failure is a reason to inspect the claim. After the repair, rerun the other cases that should still hold.*

Removing the assertion would remove the disagreement from the report, but also remove the evidence. Repair the expectation in your copy, then run it again. The original stored source remains the passing version.

## Let the compiler guard forbidden programs

Some mistakes should never reach a running test. Consider this separate program:

```osprey
fn fitsSession(minutes) = minutes <= 30

print(fitsSession(parseInt("25")))
```

The comparison requires an integer. `parseInt` returns a `Result`, because text might not describe an integer. Even though this particular literal is valid, the call still has that result type.

From `Book`, check `examples/chapter-08/failscompilation/unhandled-result.osp` with `../target/release/osprey` and `--check`. The verified diagnostic is:

```text
examples/chapter-08/failscompilation/unhandled-result.osp: type mismatch: cannot unify int with Result<int, Error>
```

The message gives a file and the incompatible types; it does not provide a line number for this example. The diagnostic spells the built-in parser's error type `Error`. The essential disagreement is between a plain integer and a result that still needs handling.

### Compiler-feedback exercise

Repair a copy so it matches the result. In the `Success { value }` arm, call `fitsSession(value)` and display its boolean result. In the `Error { message }` arm, display the message. First predict what `"25"` prints, then try `"soon"`.

The answers are `true` and `parseInt: invalid integer`, respectively. The repair chooses behavior for both outcomes; changing an annotation would not extract the integer. Keep the stored rejection fixture unchanged: it records a program the compiler must continue to reject.

The distinction matters when maintaining a suite. A failed assertion is evidence from an executed program. A compilation failure means the program never got to run its tests. Both deserve attention, but they point to different work.

## Run the checkpoint and hand off a precise task

Run `../target/release/osprey test examples/chapter-08` from `Book`. The runner discovers both `.test.osp` files, prints their results under file headings, and finishes with `# suites: 2 passed, 0 failed`. Together, they contain seven named cases. The rejection fixture is intentionally not named `.test.osp`, so discovery does not treat it as a passing test suite.

Use `make check-examples` to run the book's wider checks, including exact output and expected compiler rejection. A runner invocation checks behavior; the book command additionally keeps the printed evidence in these pages tied to the saved examples.

### Agent handoff

```text
Extend examples/chapter-08/flight-log.test.osp in Osprey
Default flavor. Add a second summary case with a different
title and Complete status. Test the exact intended text.
Add an estimate case for the empty string; preserve the
parser's actual error message. Explain each new assertion.
Keep the existing cases and the 1-to-60-minute policy.
Do not replace Result matching with a fallback.

From Book, run:
../target/release/osprey examples/chapter-08/flight-log.test.osp --check
../target/release/osprey test examples/chapter-08

Review and update the matching expected-output file for
new case names/counts, then run make check-examples.
Report the commands, outcomes, and exact expected text.
```

Review the added cases yourself. The second summary should contain the new title and `[complete]`. The empty-string case should take the error route. An agent should point to those observations, not merely report that it ran a command.

In Chapter 9, the Flight Log begins asking for work beyond a pure calculation. Algebraic effects will let a function request an operation while a chosen handler supplies its implementation. A test can supply predictable answers through that boundary. The habit stays the same: state the behavior, choose useful cases, and inspect what happened.

## Landing check

- A named test states an intended behavior; an assertion compares an observation with an independent expectation.
- `expect` takes actual then expected; `check` takes a label, expected, then actual.
- A passing run establishes only the cases and assertions that actually ran.
- Explicit states and `Result` branches provide a map of cases worth checking.
- Boundary values expose mistakes that ordinary middle values can miss.
- Pure functions keep inputs visible and results reproducible.
- Compiler rejection, runtime test failure, and output differences are distinct evidence to read before repairing code.

### Authoritative sources

- Osprey [Testing Framework](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0027-TestingFramework.md): `[TESTING-BUILTIN-TEST]`, `[TESTING-BUILTIN-EXPECT]`, `[TESTING-EQUALITY]`, `[TESTING-TAP]`, `[TESTING-EXIT]`, and `[TESTING-CLI-RUN]`.
- Osprey [Pattern Matching](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0007-PatternMatching.md) and [Error Handling](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0013-ErrorHandling.md): exhaustive cases, `Result`, and preservation of error payloads.
- Executable sources, exact output, and the rejection fixture in `examples/chapter-08/`. The chapter's deliberately false expectation and temporary behavior changes were also run and confirmed to fail; edition verification is recorded in `evidence.json`.
