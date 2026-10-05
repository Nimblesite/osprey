---
layout: page
title: "false (Keyword)"
description: "Boolean literal representing the logical value false."
---

**Description:** Boolean literal representing the logical value false.

## Example

```osprey
let isComplete = false
print(match isComplete {
    false => "Not done yet"
    true => "Done"
})
```

```osprey-ml
isComplete = false
match isComplete
    false => print "Not done yet"
    true => print "Done"
```
