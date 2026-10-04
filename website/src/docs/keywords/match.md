---
layout: page
mlTwins: manual
title: "match (Keyword)"
description: "Select a case and bind its values within that arm."
---

`match` selects a case and gives names to its values. Each arm produces the same result type. Pattern names belong to their arm: an enclosing variable with the same name keeps its original value after the match.

## Example

```osprey
type Choice = Some(int) | None
fn describe(choice) = {
    let value = 100
    let selected = match choice {
        Some(value) => value
        None => value
    }
    "${selected}:${value}"
}
print("${describe(Some(2))} / ${describe(None)}")
```

```osprey-ml
type Choice = Some int | None
describe choice =
    value = 100
    selected = match choice
        Some value => value
        None => value
    "${selected}:${value}"
print "${describe (Some 2)} / ${describe None}"
```

Both examples print `2:100 / 100:100`. The `Some` arm uses its payload; the `None` arm uses the enclosing value. Nested matches restore their enclosing bindings too. A closure created inside an arm retains the binding it captured there, and a function extracted by a pattern keeps its parameter and return types.

Patterns also select Result variants, list shapes and record fields. See the [pattern matching contract](/spec/0007-patternmatching/) for the full rules.
