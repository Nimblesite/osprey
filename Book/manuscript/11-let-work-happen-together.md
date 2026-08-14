# Chapter 11 — Let work happen together

## Reader outcome

Spawn isolated fibers, communicate through typed channels, and wait for work without shared mutable state or a separate async function kind.

## Flight Log state

Independent entries are processed concurrently and return their values through explicit communication.

## Core sections

1. Concurrency is work making progress together
2. A fiber is lighter than an operating-system thread
3. Spawn work without coloring every caller
4. Send values instead of sharing writable memory
5. Receive, await, and yield deliberately
6. Keep failures and effects visible across boundaries
7. Structured lifetime and native availability

## Compiler-feedback exercise

Cross a channel with the wrong value type or leave a required effect uncovered. Repair the boundary rather than erase type information.

## Flight Log checkpoint

Process independent entries in fibers and collect a deterministic result in a defined order.

## Planned visuals

- Isolated flight paths
- Typed channel handoff
- Parent and child lifetime

## Source map

`0011-LightweightFibersAndConcurrency`, `0036-StructuredConcurrency`, fiber corpus

