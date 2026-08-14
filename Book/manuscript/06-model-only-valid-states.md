# Chapter 6 — Model only valid states

## Reader outcome

Use records and unions to represent the real states of a problem and remove meaningless field combinations.

## Flight Log state

A loose set of flags becomes `Planned`, `Learning`, and `Complete`, each carrying only the data its state needs.

## Core sections

1. Records group facts that exist together
2. Unions list alternatives
3. Put data on the case that owns it
4. Named and positional payloads
5. Match before reading union-specific data
6. Record update returns a new value
7. Under the wing: products, sums, and algebraic data types

## Compiler-feedback exercise

Construct a record with a missing or unknown field, then use the actual checker response to repair the model rather than insert a meaningless default.

## Flight Log checkpoint

Replace status booleans with a closed union and update every renderer and test.

## Planned visuals

- Record versus union
- Impossible-state removal
- Immutable update sharing

## Source map

`0003-Syntax`, `0004-TypeSystem`, `0007-PatternMatching`

