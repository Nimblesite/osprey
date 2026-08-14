# Chapter 8 — Prove what the program does

## Reader outcome

Write tests that state meaningful behavior, cover every modeled case, and separate runtime assertions from programs the compiler must reject.

## Flight Log state

The project has useful pure functions and honest failure. The reader now turns those expectations into executable evidence.

## Core sections

1. A test is a behavior claim
2. Arrange one value, call one boundary, check one result
3. Cover every union and Result branch
4. Use grouped checks without meaningless assertions
5. Use expected output for a complete interaction
6. Keep compile-fail examples for forbidden programs
7. Run the repository's actual test command

## Compiler-feedback exercise

Write one deliberately false assertion and distinguish a test failure from a compilation failure. Repair the behavior or the expectation without deleting evidence.

## Flight Log checkpoint

Cover summary rendering, status transitions, and successful and failed estimate parsing.

## Planned visuals

- Evidence pyramid
- Edit/check/test feedback loop

## Source map

`0027-TestingFramework`, corpus conventions in `../tests/`, and repository test instructions

