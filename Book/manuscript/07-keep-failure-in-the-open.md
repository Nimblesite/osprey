# Chapter 7 — Keep failure in the open

## Reader outcome

Use `Result` for expected failure, preserve error information, and recover only where the program has a real policy.

## Flight Log state

The reader parses a text estimate. Invalid input becomes data the caller must handle rather than a hidden exit.

## Core sections

1. Expected failure belongs in the result
2. `Success` carries a value; `Error` carries information
3. Match both routes explicitly
4. Checked integer arithmetic remains honest
5. Preserve the first failure while composing work
6. Use `?:` only for an intentional fallback
7. Why ordinary failure needs no exception or panic

## Compiler-feedback exercise

Pass a `Result` where a plain value is required. Follow the compiler back to the missing policy and fix it with an exhaustive match.

## Flight Log checkpoint

Parse an estimate, show a successful duration, and retain the exact parser message on failure.

## Planned visuals

- Two-route Result
- Propagation versus handling
- Fallback decision

## Source map

`0001-Introduction`, `0007-PatternMatching`, `0013-ErrorHandling`

