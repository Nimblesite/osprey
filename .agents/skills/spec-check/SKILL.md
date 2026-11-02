---
name: spec-check
description: Find discrepancies between specs, code, tests, and docs, then bring them into line. Use when the user says "check specs", "spec audit", or "verify specs".
argument-hint: "[optional spec ID or path filter]"
---
<!-- agent-pmo:74cf183 -->

# spec-check

Bring specs, code, tests, and docs into line. The job is to find **discrepancies** and fix each one on whichever side is wrong.

`$ARGUMENTS`: optional spec ID (`AUTH-TOKEN-VERIFY`) or path fragment (`repo-standards`). Empty = all specs.

## 1. Find specs

Spec IDs are bracketed hierarchical slugs matching `\[([A-Z][A-Z0-9]*(-[A-Z0-9]+)+)\]` in `docs/**/*.md`, `SPEC.md`, `PLAN.md`, or `specs/`. Fix bad IDs as you go: no numbers (`[SPEC-001]`, `[FEAT-AUTH-01]`), no single words, and IDs in the same group sit next to each other.

## 2. Compare

For each spec section, read the section, the code that implements it (grep for the ID and the behavior), its tests, and any docs that describe it. Note every place where they disagree.

## 3. Decide which side is wrong

Use judgment. Usually it is obvious:

- **Not implemented yet:** the code doesn't do what the spec says. Keep the spec and record the gap in the owning plan or the spec's status line.
- **Spec is wrong:** the code works, but the spec asks for something that won't work or that nobody needs. Change the spec to match the code.
- **Code is wrong:** a sound spec is contradicted by the code. Fix the code, writing a failing test first.
- **Tests or docs are stale:** fix them to match the correct behavior.
- **Missing links:** add the spec ID to a comment in the implementing code and to the tests.
- **Unclear:** if you can't tell which side is right, change nothing. Flag it with ❓ in the report and ask the user to decide.

## 4. Keep specs tight

When you edit a spec, keep it short, focused, and easy for a person to read. State behavior, not implementation trivia. Delete stale, duplicated, or speculative text.

## 5. Report

List each discrepancy, the side you changed and why, and any gaps left unimplemented. End with the ❓ items, each with the options, so the user can decide.
