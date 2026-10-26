---
layout: page
mlTwins: manual
title: "String (Type)"
description: "UTF-8 text with interpolation, escape sequences and byte-based offsets."
---

String literals use UTF-8. Interpolate values with `${...}` and use escapes such as `\n` for a newline.

## Example

```osprey
let name = "Zoë"
print("Hello, ${name}! ${byteLength(name)} bytes")
```

```osprey-ml
name = "Zoë"
print "Hello, ${name}! ${byteLength name} bytes"
```

Both programs print `Hello, Zoë! 4 bytes`: `ë` occupies two bytes. [`byteLength`](/docs/functions/bytelength/) counts bytes, and [`substring`](/docs/functions/substring/) and [`indexOf`](/docs/functions/indexof/) use byte offsets. Choose code-point boundaries when slicing; [`codePointAt`](/docs/functions/codepointat/) decodes a Unicode code point at a byte offset and returns `Error` for malformed UTF-8.
