# Osprey for Zed

This extension starts the existing `osprey lsp` language server. Diagnostics,
completion, hover, navigation and formatting come from the compiler rather than
a second editor implementation.

Install Osprey so `osprey` is available on your project's PATH, or set an explicit
compiler path in Zed settings:

```json
{
  "lsp": {
    "osprey": {
      "binary": {
        "path": "/absolute/path/to/osprey",
        "arguments": ["lsp"]
      }
    }
  }
}
```

The default argument is `lsp`. Explicit arguments replace it, including an empty
list; include `lsp` when configuring the compiler. Paths and arguments are passed
directly to Zed, without a shell. `binary.env`, `initialization_options` and
`settings` are forwarded through Zed's standard LSP settings. An invalid explicit
path fails instead of silently selecting another compiler. The extension does
not download executables or bundle a second compiler version.

Run `make -C src/toolchain/zed-extension prepare` from the repository root, then
use **zed: install dev extension** and select `src/toolchain/zed-extension`.
Zed builds the Rust extension and pinned Default grammar. Development builds can
also be checked with `make -C src/toolchain/zed-extension build lint test` after
installing the `wasm32-wasip2` Rust target.

Run the real compiler acceptance suite with
`make -C src/toolchain/zed-extension stdio-test`. It uses the existing
`target/release/osprey`; select another built compiler with
`OSPREY_TEST_COMPILER=/absolute/path/to/osprey`. The suite starts isolated stdio
servers and tests diagnostics after edits, hover, completion, definitions,
references, signatures, formatting, versioned quick fixes, mixed-flavor projects,
UTF-16 positions and shutdown. It also checks manifest and canonical grammar
consistency; it does not rebuild the compiler or launch the editor.

`.osp` selects Osprey and `.ospml` selects Osprey ML. For Python-flavor sources,
select **Osprey Python** from Zed's language menu. Zed's built-in `.py` association
takes precedence over the first-line marker matcher, so a marker alone does not
switch an existing `.py` buffer. Keep `# osprey: flavor=python` in the source, or
select it in the Osprey manifest, for compiler/project discovery. Ordinary `.py`
files retain Zed's Python support. The compiler remains responsible for project
membership and authoritative syntax-flavor selection.

For a persistent project-specific association, add exact Python-flavor paths to
Zed's `file_types` setting rather than assigning every `.py` file to Osprey:

```json
{ "file_types": { "Osprey Python": ["src/model.py", "src/main.py"] } }
```

Diagnostic scope belongs in `lsp.osprey.settings`, for example
`{ "osprey": { "diagnostics": { "scope": "openFiles" } } }`. Hover infers types
from the whole project and its packages; `{ "osprey": { "analysis": { "scope":
"file" } } }` in the same place infers them from each file alone.
Zed sends workspace settings after initialization; setting only
`initialization_options` does not preserve that scope through later settings updates.

Default highlighting uses the canonical Tree-sitter queries, synchronized by
`prepare`. ML and Python-flavor buffers currently have LSP features but no
Tree-sitter highlighting or structural indentation; the Default grammar is not
applied to syntax it cannot parse. The
[developer tools checklist](../../../docs/plans/0012-developer-tools.md#zed-integration-editor-zed)
records the verified scope and limits.
