# Injected implementations

A handler is not where the implementation has to be written. `handle … in` is
an expression, so a function that returns one is a policy you pass around:
the code under test performs an operation, and the caller decides which
implementation answers it.

`storage_injection.test.{osp,ospml}` is the whole pattern in one program.
The logic is written once against a `Storage` effect, and three separate
implementations are handed to it without the logic changing, recompiling, or
knowing which one it got:

| Implementation | Installed by | What the arms do |
| --- | --- | --- |
| Real file system | `withDiskStorage` | `writeFile` / `readFile` |
| Test double | `withMockStorage` | An in-memory `Map`, plus call counters and an ordered call log |
| Read-only policy | `withReadOnlyStorage` | Refuses every write and serves one fixed entry |

```osprey
// The logic performs the operation and names no implementation.
fn appendNote(book, line) !Storage = {
    let updated = "${perform Storage.load(book)}${line}\n"
    let _ = perform Storage.save(book, updated)
    entryCount(updated)
}

// The caller chooses one, per call.
let live = withDiskStorage(|| => appendNote("notes.txt", "buy milk"))
let mocked = withMockStorage(|| => appendNote("notes.txt", "buy milk"))
```

The suite asserts the two runs answer identically, that the disk run really
wrote the bytes (they are read back with `readFile` from outside the handler),
and that the mocked run touched no file system at all — `readFile` on the
mock's paths still reports `No such file or directory`.

## Verifying calls, not just answers

Handler arms are the only place state may be mutated, which makes the test
double the natural place to record what the logic asked for. The mock counts
each operation and appends to a call log, so the suite pins the exact sequence:

```
load(notes.txt);save(notes.txt,9);load(notes.txt);save(notes.txt,22);…
```

That is the verification a real file system cannot offer, and it is what makes
the double a mock rather than a stub: the assertions cover the calls the logic
made, their order, their arguments, and the count of each.

## Why the effect is the seam

Nothing is threaded through the logic to make this work — no handle parameter,
no interface record, no constructor. `appendNote` and `countNotes` name
`Storage` in their effect row and the compiler refuses to let either reach
program entry without a handler that discharges it, so the seam is checked
rather than conventional. Swapping the implementation is swapping the caller's
one word: `withDiskStorage` becomes `withMockStorage`.

These handlers are direct-substitution handlers — no arm calls `resume` — so
each arm's value simply becomes the operation's result and the caller carries
on. See the [effects overview](../README.md) for the resuming form.
