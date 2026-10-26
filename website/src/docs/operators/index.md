---
layout: page
title: "Operators"
description: "Reference for arithmetic, comparison and pipeline operators in Osprey"
---

Arithmetic, comparison and pipeline operators are listed below. Boolean `&&`, `||` and `!` are covered by [boolean operations](/spec/0009-booleanoperations/#operators-bool-short-circuit); Result fallback `?:` is covered by [Result preservation](/spec/0004-typesystem/#result-preservation).

## [!=](not-equal/) - Inequality

Compares two values for inequality.

## [%](modulo/) - Modulo

Returns the remainder of dividing the first number by the second.

## [*](multiply/) - Multiplication

Multiplies two numbers.

## [+](plus/) - Addition

Adds numbers, concatenates strings or lists, and merges maps.

## [-](minus/) - Subtraction

Subtracts the second number from the first.

## [/](divide/) - Division

Divides the first number by the second.

## [<](less-than/) - Less Than

Checks if the first value is less than the second.

## [<=](less-equal/) - Less Than or Equal

Checks if the first value is less than or equal to the second.

## [==](equal/) - Equality

Compares two values for equality.

## [>](greater-than/) - Greater Than

Checks if the first value is greater than the second.

## [>=](greater-equal/) - Greater Than or Equal

Checks if the first value is greater than or equal to the second.

## [|>](pipe-operator/) - Pipe Operator

Passes the left value as the first argument of the call on the right.

