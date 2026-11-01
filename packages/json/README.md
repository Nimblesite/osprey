# @osprey/json

The one layer for reading and building Osprey's built-in `Json` value ([BUILTIN-JSON-TYPED]). Application code asks `Doc` instead of matching `JsonInt`, `JsonFloat` or `JsonString` by hand.

| Reader | What it gives |
| --- | --- |
| `Doc::parse`, `Doc::write` | Text to `Json` (`JsonNull` when it is not JSON) and back |
| `Doc::field`, `Doc::has` | An object member, and whether it is there |
| `Doc::text`, `Doc::number`, `Doc::decimal`, `Doc::flag` | A member as a string, integer, float or boolean, with an empty default |
| `Doc::items`, `Doc::members`, `Doc::texts` | An array's elements, an object's members, an array of strings |
| `Doc::kind`, `Doc::real` | What a value is (`null`, `bool`, `number`, `text`, `list`, `object`), and a number as a float |
| `Doc::str`, `Doc::num`, `Doc::bit`, `Doc::strings` | Members and arrays for building a document |

Every reader is total: it reads a missing or mistyped member as its empty value. That suits trusted state such as a UI machine document. Validate hostile input with the built-in `Result` readers (`jsonText`, `jsonInt`, `jsonField`) instead.

`Doc` lives in the `ui` namespace (`import ui::Doc`). A project lists this package's `src` folder as a source root, beside `@osprey/native` and `@osprey/graphics`:

```toml
source_roots = ["src", "../../packages/native/src", "../../packages/graphics/src", "../../packages/json/src"]
```
