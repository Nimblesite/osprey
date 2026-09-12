# Chapter 3 — Let the compiler work out the types

The Flight Log already contains text, whole numbers, and a yes-or-no value. You have used them without writing a type beside every name. The compiler has still been checking your work.

This chapter makes that checking visible. Start with a small heading for the log:

```osprey
fn heading(name) = "Flight Log: " + name

fn main() = {
    let owner = "Mika"
    heading(owner) |> print
}
```

Save this as `heading.osp`, or use `examples/chapter-03/heading.osp` from the book. Run `osprey heading.osp --check`, then `osprey heading.osp --run`. The output is `Flight Log: Mika`.

There is no written parameter type, but `heading` cannot accept just anything. The string on the left of `+` requires text on the right. That requirement connects the function body to its parameter and then to the argument supplied by its caller.

By the end of the chapter, you will be able to follow that connection, recognise an annotation that adds useful information, and read a mismatch without changing types at random. The Flight Log will gain a record that groups its five related facts.

## Read a type as a set of possibilities

A **type** describes the values an expression can produce and the operations available to use them. A string contains text. An integer represents a whole number. A boolean has two possible values: `true` and `false`.

The source often supplies the first clue:

| Source value | Type | Meaning in the Flight Log |
|---|---|---|
| `"Mika"` | `string` | The learner's name |
| `"Build a small Osprey CLI"` | `string` | The learning goal |
| `true` | `bool` | Whether the plan is active |
| `3` | `int` | The planned session count |
| `25` | `int` | The minutes in each session |

The quotes matter. `3` is an integer; `"3"` is a string containing one digit. They can look the same when printed, but their types describe different uses. Replacing an integer with text is more than changing how a number looks.

Types also have limits. Both a learner's name and a learning goal are strings. A type checker cannot discover that you accidentally exchanged those two strings merely by examining their types. Meaningful names, sensible data models, and behavior checks still matter.

Likewise, an `int` field does not by itself require a positive session count. A negative whole number still has type `int`. Chapter 4 introduces decisions you can use to check rules such as “a plan needs at least one session.” For now, separate two questions: does this value have the required kind, and does it make sense for this particular problem?

## Follow the inference trail

The compiler works out missing types from the relationships in your source. That process is **type inference**. It is checking with information gathered from the program, rather than asking you to repeat that information in annotations.

Read `heading` one connection at a time. `"Flight Log: "` is a string literal. Here `+` joins strings, so `name` must also be a string. The joined value is a string, which makes the function's result a string. A call to `heading` must respect that relationship.

![String concatenation requires a string parameter; heading therefore accepts a string and produces a string.](assets/diagrams/03-inference-trail.png)

*Figure 3.1 — The body supplies enough information to check the parameter and result. No annotation is needed to make those checks happen.*

You can read the function's type as “string in, string out,” sometimes written `(string) -> string` in type displays. That is a description of the function. It is not a line you need to paste above it.

The compiler does not run the program once, observe Mika's name, and guess what future calls might do. It checks the source before the program runs. Changing the name to `"Rowan"` preserves the relationship because the new argument is still a string.

The trail is a way to explain the constraints, not a promise that the compiler visits lines in that order. Information can come from a body, a call, a field declaration, or another connected expression. You do not need to imitate the compiler's internal steps. You need to identify the requirements that must agree.

### Try it: change a value without changing its type

Replace `"Mika"` with your own name. Predict the entire output line, then run the program. The heading's prefix should stay unchanged, and the name should change exactly once.

Now change only the prefix to `"Learning with "`. The parameter and result are still strings. This is a useful distinction: editing a value or a function's behavior does not always change its type. Type checking establishes compatible combinations, while an output check establishes which text your function actually produces.

## Notice which operation creates the requirement

Chapter 2 used string interpolation to display several different kinds of value. An integer or boolean between `${` and `}` can be rendered as text. That does not mean an integer or boolean becomes an acceptable argument to every function returning a string.

In `heading`, the parameter is used directly as an operand of string concatenation. The expression `"Flight Log: " + name` requires `name` to be text. An expression such as `"Flight Log: ${name}"` asks interpolation to render the supplied value instead. The expressions can produce the same visible line for a string argument while placing different requirements on their parameters.

This is why “both versions printed the same thing” does not prove their types are interchangeable. The body determines what callers are allowed to supply. Before changing an operation to silence an error, decide which callers the function is supposed to accept.

For this heading, a name is text. Keeping concatenation gives that decision a direct expression in the body. If you were writing a general display function, accepting other kinds of value might be deliberate. The important part is to make the decision from the function's purpose.

The same care applies to annotations. A program can still compile after you remove a written type while accepting calls it previously rejected. Successful compilation alone is therefore not proof that the annotation said nothing. You must consider the inferred contract as well as the current output.

## Put written types where they add information

An **annotation** is a type written in the source to constrain an expression, parameter, or result. It must agree with the program. It does not convert a value simply because you would prefer a different type.

Osprey reports a redundant-annotation warning when it can prove that a written annotation adds no information to the inferred types. The heading already determines that its parameter and result are strings, so leave both annotations off. The short definition remains fully checked.

Now consider the five facts belonging to a learner. A record gives those facts named fields:

```osprey
type Learner = {
    name: string,
    goal: string,
    isActive: bool,
    sessionCount: int,
    minutesPerSession: int
}
```

These field types define the record's shape. There is no field value in this declaration from which the compiler could infer that `sessionCount` means an integer. You are specifying what a `Learner` contains, before constructing a particular learner.

The colon connects a field name to its type: `isActive: bool` says that the field contains a boolean. Spell the built-in types exactly as shown. `int` and `Int` are not interchangeable spellings.

![Keep the field types that define Learner; omit function annotations when the body already determines the same types.](assets/diagrams/03-annotation-choice.png)

*Figure 3.2 — A record declaration supplies the field contract. An inferred function uses that contract without repeating it on every parameter.*

The distinction is between defining the data and repeating a conclusion. When a function constructs `Learner { ... }`, the declared field types constrain the values supplied for those fields. The surrounding function can often keep its parameter and return types inferred.

In less constrained code, an annotation can be necessary. A value with no useful context, or a function intended to accept fewer kinds of input than its body requires, may need a written type. Keep that information. The habit to build is asking what a type declaration or annotation contributes, rather than erasing every colon or adding one beside every name.

You will use the record below as a boundary: the place where the Flight Log says what facts a learner must carry. Chapter 6 develops record design further. You only need to read these five fields and their uses here.

## Reuse a function without losing the relationship

Some function bodies do not require one particular input type. This one returns exactly what it receives:

```osprey
fn identity(value) = value

fn main() = {
    let owner = identity("Mika")
    let sessionCount = identity(3)
    let isActive = identity(true)
    print("${owner} | ${sessionCount} | ${isActive}")
}
```

The complete example is `examples/chapter-03/identity.osp`. Checking and running it produces `Mika | 3 | true`.

There is one definition of `identity`. The string call returns a string, the integer call returns an integer, and the boolean call returns a boolean. The body has preserved a relationship: for each use, the input and output have the same type.

This kind of reuse is called **polymorphism**. You may see its relationship written `<T>(T) -> T`. Read `T` as a type variable: a name for a type that is allowed to differ between uses. The two occurrences of `T` describe the same type within one use. They do not mean “choose an unrelated type for each position.”

That is a useful promise. If the input to this function is a string, its result remains a string. You can pass that result to `heading` without first recovering what kind of value it was.

No explicit generic declaration is needed for this example. The compiler infers the relationship from `value` appearing as both the parameter and the result. Later code can use richer generic declarations; there is no need to learn their syntax before understanding this basic behavior.

### A type variable is different from `any`

Inference does not replace the missing annotations with `any`. Osprey's explicit `any` type hides information about the original concrete type for compatibility with code that needs that flexibility. It has its own restrictions on using the value and accessing fields.

Inferred `identity` retains the connection between its argument's type and its result's type. That connection is precisely what you want here. Replacing it with `any` would discard useful information rather than explain the function more clearly.

There is no need to explore `any` further in this chapter. Remember the practical distinction: “works at several types while preserving their relationships” is stronger than “the concrete type is hidden.” Leave the inferred definition alone.

## Read a mismatch through the call

Now make the promised mistake in a separate file. Supply a boolean to the string-based heading:

```osprey
fn heading(name) = "Flight Log: " + name

fn main() = heading(true) |> print
```

From the `Book` directory, check the stored rejection example:

```sh
../target/release/osprey examples/chapter-03/failscompilation/type-mismatch.osp --check
```

The development compiler used to verify this chapter rejects it with this exact message:

```text
examples/chapter-03/failscompilation/type-mismatch.osp: type mismatch: cannot unify string with bool
```

This diagnostic gives the file and the conflicting types. It does not include a line or column for this example. If you use a different file path, that path will appear instead. Do not look for a location or an “expected” label that the message did not supply.

**Unify** means making the connected type requirements agree. Here the string concatenation requires text, while the call supplies `true`, a boolean. These requirements cannot both hold. Nothing needs to crash at runtime for the compiler to identify the disagreement.

![Find the reported file, identify the string and bool conflict, then inspect heading(true) and the concatenation in heading.](assets/diagrams/03-mismatch-locator.png)

*Figure 3.3 — The diagnostic names the disagreement. Following the argument into the function explains which requirement caused it; this diagram is a reading guide, not a terminal capture.*

Start at `heading(true)`. Follow the argument into the parameter `name`, then examine how `name` is used. The string operation explains why the call is invalid. You can now make a repair based on the intended meaning.

For the learner's heading, pass a name such as `"Mika"`. Quoting `true` would produce the string `"true"`, which has the right type but is probably the wrong learner name. Changing the body to interpolation would permit a broader use, which is a different design decision. Adding an annotation that insists the parameter is boolean would contradict the concatenation rather than repair it.

The message should lead you back to the problem you are solving. Once repaired, check again and run the program. A clean type check proves the connection is compatible; the output tells you whether you supplied the intended name.

## Flight Log checkpoint

The next Flight Log groups its five facts into a `Learner`. Its summary shows the session count and duration separately so you can see the fields being checked. Calculating a planned total remains a separate job from defining those inputs.

Use the complete source in `examples/chapter-03/flight-log.osp`:

```osprey
type Learner = {
    name: string,
    goal: string,
    isActive: bool,
    sessionCount: int,
    minutesPerSession: int
}

fn heading(name) = "Flight Log: " + name

fn makeLearner(name, goal, isActive, sessionCount, minutesPerSession) =
    Learner {
        name: name,
        goal: goal,
        isActive: isActive,
        sessionCount: sessionCount,
        minutesPerSession: minutesPerSession
    }

fn summary(learner) =
    "${heading(learner.name)} | goal: ${learner.goal}" +
    " | active: ${learner.isActive}" +
    " | sessions: ${learner.sessionCount}" +
    " | minutes each: ${learner.minutesPerSession}"

fn main() = {
    let learner = makeLearner(
        "Mika", "Build a small Osprey CLI", true, 3, 25
    )
    summary(learner) |> print
}
```

`Learner { ... }` constructs a record value. In `name: name`, the first `name` is the field label and the second is the parameter supplying that field. The same pattern connects each remaining parameter to its declared field type.

The result of `makeLearner` is a `Learner`. Its parameters are inferred from the construction: two strings, a boolean, and two integers. `summary` reads fields using a dot, as in `learner.goal`. There is one declared record shape in this example, so its field uses can also be resolved without a parameter annotation.

Notice what moved. `main` supplies the five values at construction, then passes one record to `summary`. A named group now travels between the functions. The field declaration keeps checking the group's contents even though the functions have no written parameter or return annotations.

Check and run the checkpoint from `Book` with `../target/release/osprey examples/chapter-03/flight-log.osp --check` and the same command ending in `--run`. The complete output is:

```text
Flight Log: Mika | goal: Build a small Osprey CLI | active: true | sessions: 3 | minutes each: 25
```

In a copy, replace the integer argument `3` with the string `"3"`. Predict whether the record construction can satisfy `sessionCount: int`, then check it. Restore the integer before continuing. This time the requirement comes from the field declaration rather than a string operation, but you follow the disagreement in the same way.

### Agent handoff

Use this task if you want help adapting your own Flight Log:

```text
Update flight-log.osp in Osprey Default flavor.
Group the learner name, goal, active status, session count,
and minutes per session into one Learner record.
Use the Chapter 3 field types and infer the surrounding
function parameters and results. Keep heading string-based.
Build one summary string and print it through one path.
Do not introduce any or change a type just to silence an error.

Run:
osprey flight-log.osp --check
osprey flight-log.osp --run

Report the exact output. Explain how each makeLearner
parameter gets its type from the field it supplies.
```

Review that explanation against the source. An agent should be able to point from each parameter to a field, not merely claim that the program compiles. From the book directory, `make check-examples` checks all completed examples, compares their output exactly, and verifies the stored mismatch is rejected with its recorded message.

## Landing check

- A type describes possible values and compatible operations; domain meaning still needs your judgment.
- Literals, operations, calls, and declared fields supply information for inference.
- A function can be fully checked without written parameter or return types.
- Record field declarations define a useful boundary; redundant annotations repeat an inferred conclusion.
- Polymorphic `identity` preserves the input/output relationship separately at each use.
- A mismatch becomes easier to repair when you follow the supplied value into the requirement it cannot meet.

The Flight Log now carries a small, checked group of facts. Chapter 4 uses those facts to make explicit decisions and produce a value for each possible case.

### Authoritative sources

- Osprey [Type System](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0004-TypeSystem.md): Hindley–Milner inference, record fields, `[TYPE-ANY]`, `[TYPE-ANNOTATION-CHECK]`, and `[TYPE-ANNOTATION-REDUNDANT]`.
- Osprey [Syntax](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0003-Syntax.md) and [Function Calls](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0005-FunctionCalls.md): record construction and Default-flavor calls.
- Osprey [String Interpolation](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0006-StringInterpolation.md): rendering values inside strings.
- Executable source, exact output, and the compiler-rejection example in `examples/chapter-03/`, checked with the edition's development compiler. The development version remains unpinned for publication; build identity is recorded in `evidence.json`.
