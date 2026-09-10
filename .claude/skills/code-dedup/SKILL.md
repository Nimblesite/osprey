---
name: code-dedup
description: Finds duplicated code and dead code with deslop, then merges or removes the worst of it. Use when the user says "deduplicate", "find duplicates", "remove dead code", "DRY up", or "code dedup".
---
<!-- agent-pmo:74cf183 -->

# Code Dedup

Find duplication with **deslop**, then use judgement to merge only what is worth merging.

## Judgement first

Deslop measures structural repetition. It cannot tell whether collapsing two blocks makes the code better or worse. **You decide.** A report with 476 clusters does not mean 476 edits — it means the five or ten that matter. `merge-plan` returns verdict `ai_or_human` for anything non-mechanical: that is the tool handing you the call.

**Merge** real copy-pasted logic — a block differing only in a symbol or a literal, 3+ near-identical call sites, one algorithm restated in two modules.

**Leave** anything else, especially:

- **Data, not logic** — constant tables, per-platform SDK/triple/runtime arms, doc rows. Merging buries the values the reader came for.
- **Structural coincidence** — `structural_only` CST walks and `match` shapes: same skeleton, different meaning.
- **Already factored** — only thin wrappers remain over a real helper.
- **Merges needing a new bool/enum parameter** to make one function do two jobs.

A wrong merge is worse than a duplicate.

## Tools

Prefer the MCP — live index, no rescan cost:

| Tool | Use |
|---|---|
| `mcp__deslop__duplicates` | **Start here.** Clusters worst-first by `mass`. `{detail:"summary", limit:20}`; `include_per_file:true` for a per-file table, `path_contains` to scope. |
| `mcp__deslop__cluster-by-id` | Every occurrence path + byte range for one `id`. |
| `mcp__deslop__compare-pair` | The **only** pair evidence: two `{path,start_byte,end_byte}` in, `text_identity` and `structural` out. |
| `mcp__deslop__merge-plan` | Mechanical plan for a cluster. `ai_or_human` → hand-edit. |
| `mcp__deslop__find-similar` | Call **before writing new code** (CLAUDE.md mandates it). |
| `mcp__deslop__rescan` | Refresh after edits. |

A cluster's `kind` is the *weakest* pair against the canonical — **never assume two members match each other**. `compare-pair` the exact ranges you intend to merge.

No MCP? Use the installed CLI — what `make _deslop` runs:

```bash
deslop . --nohtml --nojson --output "$PWD/target/deslop-report" --log-to-console --log-level error --no-color
```

Exit `3` means over the ceiling in `.deslop.toml`. Check `deslop --version`; install from <https://deslop.live/docs/for-ai/> only if the binary is missing, matching the version pinned in `.github/workflows/ci.yml`. Never upgrade to move a number — the MCP server (`tool_version: 0.0.0-dev`) and the pinned CLI disagree, so **find** with the MCP and **measure** with `make _deslop`.

## The ratchet

**CI must never allow duplication to increase.** `max_duplication_percent` is a ratchet, not a budget.

- **Never raise it.** Over the ceiling means you added duplication — remove it. The number is not the thing to edit.
- **Lower it after every round** to the fresh `make _deslop` measurement. Slack above the measurement lets the next clone land free.
- **A branch may not measure above `main`** — compare with the *same* CLI version, or the version gap alone will convict or exonerate falsely.
- `.deslop.toml`'s comment block records the ratchet history. Extend it when you lower the ceiling; if that history and the live value disagree, someone raised it — **report a gate violation**.

## Process

- **Baseline.** `make _deslop` — record the percentage you start from.
- **Gather.** `mcp__deslop__duplicates`, worst first. `cargo clippy --all-targets --all-features` is the whole dead-code scan (`dead_code`/`unused_*` are denied workspace-wide); grep the repo before deleting anything it flags.
- **Triage.** Apply the judgement above. Record each keeper's two locations and intended helper, and each rejection's reason so nobody re-litigates it. `.deslop.toml` scopes the gate *by path*, so inline `#[cfg(test)]` and `#[path = "…"]` modules still inflate the number — an artifact, not a task.
- **Apply.** One merge at a time, smallest diff that removes the duplication. Keep public and `pub(crate)` signatures intact so no caller has to change.
- **Verify.** `make lint`, then `make _deslop`. Report what merged, what you left and why, and duplication vs baseline.

## Rules

- **Three similar lines is fine** — merge at >10 shared lines or 3+ copies.
- **Edit in place.** Never leave a parallel version of anything behind.
- **When in doubt, leave it.**
