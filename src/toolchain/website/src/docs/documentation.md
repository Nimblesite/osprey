---
layout: page.njk
mlTwins: false
title: Documenting your modules
description: Generate HTML API documentation, add Markdown guides and custom CSS, and run executable examples.
---

{% from "native-capture.njk" import capture %}

Osprey builds API documentation directly from your modules and the compiler's
inferred types. Both Default and ML sources support documentation comments.

## Write comments and examples

In Default source, put `///` above a declaration. Use `//!` at the beginning of
a file, namespace, or module to document that scope.

~~~~osprey
//! Greetings for command-line applications.

/// Greets a reader by name.
///
/// # Parameters
/// - name: the reader's display name
/// # Returns
/// The greeting text.
/// # Examples
/// ```osprey
/// print(greet("Osprey"))
/// ```
/// ```output
/// Hello Osprey!
/// ```
fn greet(name) = "Hello " + name + "!"
~~~~

~~~~osprey-ml
//! Greetings for command-line applications.

(** Greets a reader by name.

# Parameters
- name: the reader's display name
# Returns
The greeting text.
# Examples
```osprey
print (greet "Osprey")
```
```output
Hello Osprey!
```
*)
greet name = "Hello " + name + "!"
~~~~

ML uses `(** ... *)` above declarations and the same `//!` inner comments.
Example code is interpreted in the source file's flavor. The structured sections
also support `# Raises`, `# See also`, `# Since`, and `# Deprecated`, plus aliases
such as `@param`, `@return`, and `@author`. Use `[Name]` to refer to another API.

{{ capture("/assets/images/guides/api-index.png", "Generated JSON-library reference · midnight theme · modules and searchable APIs.", "Generated JSON-library reference · midnight theme · modules and searchable APIs.", 1440, 1000) }}

## Generate a site

```sh
osprey --docs ./my-project --docs-dir ./site --docs-format html
```

Open `site/index.html` or serve it statically. Browse modules, public members, inferred signatures, examples, search and built-ins. Single `.osp`/`.ospml` files and libraries without entry points also work.

Function pages show which effect operations their callers must provide, including requests made through helpers. They also list runtime operations and explicitly flag callbacks whose effects remain unresolved. These are the same details shown in editor hover, signature help and completion. Written effect annotations remain visible as bounds; they are separate from the operations the compiler finds.

The **State boundaries** page (`api/project/state-boundaries.html`) lists state owners, private-cell counts and exported effects, including private-module owners. It hides cell names, initial values and private helpers. Editor/CLI state warnings show the same ownership information. Handler installation creates fresh cells; imports do not share an instance.

Choose a built-in theme:

| Theme | Appearance |
| --- | --- |
| `osprey` | Blue reference layout with module cards and an article outline |
| `midnight` | Dark console layout with compact spacing and monospaced details |
| `paper` | Editorial layout with serif headings and ruled lists |

```sh
osprey --docs ./my-project --docs-dir ./site --docs-format html --docs-theme midnight
```

Each template has its own typography and layout. The landing page starts with
your modules and guides. Expand navigation groups to browse the reference, or
press `/` to search. Article outlines link to sections, and code examples retain
their exact text when copied.

Omit the source to export the built-in reference alone. Omit `--docs-format html`
to generate Markdown with front matter for use in an existing documentation site.

{{ capture("/assets/images/guides/api-function.png", "Generated function page · inferred signature, documentation and parameters.", "Generated function page · inferred signature, documentation and parameters.", 1440, 1000) }}

## Add guides and branding

```sh
osprey --docs ./my-project --docs-dir ./site --docs-format html \
  --docs-page ./guides --docs-page ./release-notes.md --docs-css ./brand.css
```

Markdown pages join the site's navigation and search. Directories are scanned
recursively. Tables, fenced code, footnotes, task lists and ordinary Markdown
links are supported. Raw HTML is shown as text.

Custom styles load after the theme. For example, `brand.css` can change the
accent while retaining the responsive layout:

```css
:root {
  --accent: #7145b8;
  --accent-soft: #f0e9fa;
}

.article { max-width: 76ch; }
```

Pass multiple `--docs-css` options to layer styles in order. The generated
`assets/theme.css` is also a complete starting point for a custom design.

## Test the examples

```sh
osprey ./my-project --doctests
osprey ./my-project --doctests --memory=arc
osprey ./my-project --doctests --target=wasm32 --memory=none
```

Every example is type-checked. An immediately following `output` fence makes
it executable and asserts its stdout exactly, including spaces. An example
without an output fence is checked without running. Each example has its own
bindings and can use the documented declaration's module scope. Application
entry code is not run automatically; examples may call `main` explicitly.

A failed example produces a diagnostic and a failing exit status. Runnable
examples have a 30-second execution limit; set `OSPREY_DOCTEST_TIMEOUT_MS` to
another positive duration in milliseconds when needed. The WASM mode uses
`wasmtime`, or the executable selected by `OSPREY_WASM_RUN`.
