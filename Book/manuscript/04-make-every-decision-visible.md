# Chapter 4 — Make every decision visible

**Chapter outline.** The teaching plan below has been checked against the current language; the full lesson and checkpoint are still to be written.

## Reader outcome

Use `match` to make decisions, unpack data, and cover every case of a boolean, `Result`, or closed union. Use a catch-all for open scalar domains such as integers, whose exhaustiveness the compiler does not prove.

## Flight Log state

The project gains explicit `Planned` and `Complete` cases. Rendering the status requires handling both.

## Core sections

1. A decision is an expression with a result
2. Match booleans and exact scalar values
3. Use `_` when every remaining value shares one answer
4. List the cases with a union
5. Unpack one payload with a pattern
6. Let exhaustiveness stop a missing case
7. Reject unreachable branches instead of hiding them

## Compiler-feedback exercise

Add a union case without updating the renderer. Use the real exhaustiveness error, then add the smallest meaningful branch.

## Flight Log checkpoint

Render every project state to a string and test each branch.

## Planned visuals

- Exhaustive decision fan-out
- Pattern and payload anatomy
- Missing-case gate

## Source map

`0003-Syntax`, `0004-TypeSystem`, `0007-PatternMatching`
