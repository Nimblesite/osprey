---
layout: page.njk
title: Documenting your modules
description: Generate HTML API documentation, add Markdown guides and custom CSS, and run executable examples.
---

Osprey builds API documentation directly from your modules and the compiler's
inferred types. Both Default and ML sources support documentation comments.

## Write comments and examples

In Default source, put `///` above a declaration. Use `//!` at the beginning of
a file, namespace, or module to document that scope.

````osprey
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
````

ML uses `(** ... *)` above declarations and the same `//!` inner comments.
Example code is interpreted in the source file's flavor. The structured sections
also support `# Raises`, `# See also`, `# Since`, and `# Deprecated`, plus aliases
such as `@param`, `@return`, and `@author`. Use `[Name]` to refer to another API.

## Generate a site

```sh
osprey --docs ./my-project --docs-dir ./site --docs-format html
```

Open `site/index.html` or publish the directory with any static file server.
The site includes public APIs, module descriptions, inferred signatures, examples,
search, and the built-in reference. Single `.osp` and `.ospml` files work too.
Library projects can generate documentation without an application entry point.

Choose a built-in theme:

| Theme | Appearance |
| --- | --- |
| `osprey` | Warm light background and green accents |
| `midnight` | Dark surfaces and blue accents |
| `paper` | Minimal white pages suitable for printing |

```sh
osprey --docs ./my-project --docs-dir ./site --docs-format html --docs-theme midnight
```

Omit the source to export the built-in reference alone. Omit `--docs-format html`
to generate Markdown with front matter for use in an existing documentation site.

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

article { max-width: 76ch; }
```

Pass multiple `--docs-css` options to layer styles in order. The generated
`assets/theme.css` is also a complete starting point for a custom design.

## Test the examples

```sh
osprey ./my-project --doctests
osprey ./my-project --doctests --memory=arc
osprey ./my-project --doctests --target=wasm32
```

Every example is type-checked. An immediately following `output` fence makes
it executable and asserts its stdout exactly, including spaces. An example
without an output fence is checked without running. Each example has its own
bindings and can use the documented declaration's module scope. Application
entry code is not run.

A failed example produces a diagnostic and a failing exit status. Runnable
examples have a 30-second execution limit; set `OSPREY_DOCTEST_TIMEOUT_MS` to
another positive duration in milliseconds when needed. The WASM mode uses
`wasmtime`, or the executable selected by `OSPREY_WASM_RUN`.
