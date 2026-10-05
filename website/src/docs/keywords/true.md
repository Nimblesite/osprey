---
layout: page
title: "true (Keyword)"
description: "Boolean literal representing the logical value true."
---

**Description:** Boolean literal representing the logical value true.

## Example

```osprey
let isReady = true
print(match isReady {
    true => "Ready!"
    false => "Wait"
})
```

```osprey-ml
isReady = true
match isReady
    true => print "Ready!"
    false => print "Wait"
```
