---
layout: page
mlTwins: manual
title: "import (Keyword)"
description: "Import declaration keyword. Used to bring modules and their exports into the current scope."
---

`import` brings a logical namespace, module or selected public members into scope. Files can use either syntax flavor; paths on disk do not determine imported names. Use `::` for module members and `.` for record fields.

## Example

```osprey
namespace billing {
    module Tax {
        export fn double(cents) = wrapMul(cents, 2)
    }
}
namespace app {
    import billing::Tax as T
    import billing::Tax::{double as twice}
    fn main() = print("${T::double(21)} ${twice(21)}")
}
```

```osprey-ml
namespace billing
    module Tax
        export double cents = wrapMul cents 2
namespace app
    import billing::Tax as T
    import billing::Tax
        double as twice
    main () = print "${T::double 21} ${twice 21}"
```

Both programs print `42 42`. The total `wrapMul` builtin makes the arithmetic policy explicit.

Import a whole module with `import billing::Tax`, select members as above, or choose an alias with `as`. Quoted namespace labels such as `"billing/api"` need an alias for a whole import. Private members and private intermediate modules remain inaccessible.

Wildcard imports require `[modules].allow_wildcard_imports = true`; they are always rejected for state modules and namespaces containing one. Importing a state module does not create an instance or install its handler.

The editor offers repairs for a missing quoted-import alias and mistaken dot qualification when the proposed edit makes the live project compile. It checks the current unsaved files before offering an edit. See the [module specification](/spec/0025-ModulesAndNamespaces/) for signatures, ownership rules and remaining implementation limits.
