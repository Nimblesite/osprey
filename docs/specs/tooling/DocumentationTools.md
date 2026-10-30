# Documentation tools

The corresponding DocumentationComments contract is [here](../language/0026-DocumentationComments.md).

## Example extraction `[DOC-DOCTEST-HARNESS]`

Inside a recognized examples section, each fenced block labeled `osprey`,
`osprey-ml` or `ospml` becomes a `DocExample`. The label never selects the
flavor: an example compiles in the flavor of the file that documents it. The next
fence labeled `output`, separated from the example by nothing but blank lines,
supplies `expected_output` and sets `run` to `true`; without one,
`expected_output` is absent and `run` is `false`.

An `output` fence that no example precedes — one after an unlabelled fence, or
after prose — compares nothing. It is recorded as a problem with the
documentation rather than dropped, and `--doctests` fails on it, naming the
declaration: an expected output that is never checked must not read as a pass.

`osprey <file-or-project> --doctests` validates the original sources, then checks
each example independently using the owning file's resolved flavor. A project
may contain both flavors and does not need an application entry for documentation.
The example has access to the documented declaration's lexical scope, including
private module helpers. Application entry statements and `main` are not run
automatically; an example may call a documented `main` explicitly.
Bindings from one example never become visible to another.

Every example is type-checked, including declarations it does not call. An
example without an output fence is compile-only. A runnable example compiles
through the ordinary native or `wasm32` backend, must exit successfully, and
must produce the exact bytes in the output fence plus its final newline. An
empty output fence expects no stdout bytes. Spaces and blank lines are
significant. A failure names the source, declaration, and example ordinal;
the command exits unsuccessfully if any example fails or any examples section
holds an orphaned `output` fence.

Native examples default to `gc` and support `--memory=gc|arc|none`; ARC execution additionally
requires exactly one zero-live-object exit sentinel. `wasm32` defaults to ARC, rejects tracing GC, and uses `OSPREY_WASM_RUN` (default `wasmtime`) as its executable host.
Each runnable example has a 30-second execution limit, configurable through a
positive `OSPREY_DOCTEST_TIMEOUT_MS`. Compilation is outside this execution limit.

The existing corpus harness runs documentation examples in its native allocator
passes and its WASM pass. It visits every file holding an example fence under any
accepted label or an `output` fence, rejects malformed result summaries and output
drift, and requires at least six successful examples across the two source flavors.

## Markdown rendering `[DOC-EXPORT]`

`DocComment::render_markdown` emits the summary, body, and populated structured
sections in model order. LSP declaration hovers append this rendering beneath
the declaration signature or type ([LSP-HOVER-DOCS]). Authorship is rendered
after the version section when present.

`osprey --docs --docs-dir <directory>` exports the built-in reference. Supply
`--source <file-or-project>` or a positional source to include user APIs. The
exporter validates syntax, module contracts, and types before writing pages.
Files inherit the same flavor selection as compilation; projects select the
flavor per source. Library projects do not need an application entry.

User pages include file and namespace documentation, module documentation,
public functions and values, types, effects and their operations, externs, and
module signatures. Public declarations appear even without comments. Module
visibility is the compiler's finalized surface, including signature ascription;
private members and opaque type representations are excluded. Public type pages
show their representation, fields and variants. Function signatures come from
the editor's inferred signature and retain generic binders and the declared
effect row. The exporter replaces the editor's resolved row with the author's
source-local row once. ML source signatures and example fences are
presented in the ML flavor.

Function pages also include the same inferred effect requirements as editor hover ([LSP-EFFECT-REQUIREMENTS](0020-LanguageServerAndEditors.md#inferred-effect-requirements-lsp-effect-requirements)). This includes transitive required operations, separate host runtime operations and explicit unresolved-callback status. It is distinct from a written upper bound and from the source operations listed elsewhere on the page. Only the compiler's summary supplies these facts; the documentation exporter must not infer a second answer by walking source calls.

A declaration with no comment still states facts about itself, and its page
carries them: the effects it performs, each linked to the effect's own page; the
result type where one was written; and the file and line it is declared on. An
ML page also lists the parameter names, which a curried signature line does not
carry — the names are read through the lambda chain currying lowers a clause
head into, so a three-argument function lists three arguments. A Default
signature line already names every parameter, so no list repeats it. A section
the author wrote themselves is never duplicated by a derived one.

Namespace, module and effect pages list their immediate public members with
links, declaration kinds and documentation summaries. A member with no summary
is described by its own signature, unqualified by the owner the listing already
names. State modules retain the `state` qualifier, and static effects retain
`static` in their signatures.

Markdown is the default format. Built-ins live under `functions/`, user APIs
under `api/`, and additional pages under `guides/`. Names are made safe and unique
on case-insensitive filesystems; `api/index.md` is reserved for the API listing.
Multiple contributions to a namespace share its page. File documentation keeps
the source's relative path identity. Regeneration removes obsolete pages recorded
in the format's manifest, while the historical built-in `functions/*.md` tree
remains wholly generated. Unrelated files outside that tree are preserved.
Duplicate output paths, malformed manifests, and symlinks inside the destination
are rejected before writing the output set.

### State ownership `[DOC-STATE-BOUNDARIES]`

Projects containing state modules generate `api/project/state-boundaries.md` (or `.html`) from the compiler's [state inventory](../language/0025-ModulesAndNamespaces.md#state-boundary-inventory-modules-state-inventory). The page lists every state owner in deterministic qualified-name order, including private or empty modules, with private cell counts and exported owned effects. This architecture inventory intentionally names private owners but exposes no cell names, initializers or private helper APIs. It explains per-handler instance creation, appears in API navigation and offline search, and is removed on regeneration when the project no longer contains state modules. Public declaration filtering remains unchanged.

### HTML sites `[DOC-EXPORT-HTML]`

`--docs-format html` produces a complete static site with a root `index.html`,
API and built-in pages, navigation, search, and local styles. It needs no website
framework, build step, CDN, or network connection. Navigation and search work
when opened directly from the filesystem or served from a static server, including
under a URL prefix: every generated reference is relative.

A declaration page opens with its signature in a labelled disclosure rather than
a bare block of code, expanded so the reference answers on arrival and
collapsible once it has been read. Its breadcrumb names every enclosing scope
this export documents and links each one, so a reader who arrived from search
can reach the module that owns what they are reading.

Layout adapts to mobile screens and keyboard navigation. Navigation precedes the
article in source order, which is what a screen reader needs and what would
otherwise push a phone reader past the whole reference before the page they
opened. It is therefore a disclosure: expanded on a wide screen, collapsed on a
narrow one, operable from the keyboard, and expanded in the markup itself so the
links remain present when scripting is off.

The renderer supports CommonMark plus tables, footnotes, task lists and
strikethrough. Raw HTML in Markdown is displayed as text; it cannot inject scripts
or markup into generated pages. Source code stays escaped. A link or image
destination in a scheme that executes in the reader's browser — `javascript:`,
`data:`, `vbscript:`, however spelled — is replaced with an inert one; the link
text is still shown.

Headings receive unique anchors, including repeated headings and names that
already contain numeric suffixes. Generated anchors cannot collide with page
landmarks, explicit heading IDs or footnotes. Heading attributes retain IDs and
classes for navigation and styling; executable attributes are discarded.

Internal Markdown links point to generated HTML pages. A destination is put
through the same slugging the exporter used when it wrote the target, so a link
naming the author's own filenames resolves whether it was written with spaces,
percent-encoded, relative to a parent directory, or with an anchor. A
destination carrying any other extension is an asset and is left as written.

A resolvable documentation symbol link points to the matching declaration. A
declaration answers to its qualified name and to every suffix of it, in the
`::` and the dotted spelling, so `[Money.format]` and `[format]` both reach
`shop::Money::format`.

Resolution starts in the scope the comment was written in and widens outwards,
the way name resolution does in the language: `[helper]` inside `shop::A::read`
is `shop::A::helper` where that exists, even when an unrelated `shop::B::helper`
also does. Only where no enclosing scope owns the name does a match anywhere in
the export apply, and then only if exactly one declaration answers to it. A name
two unrelated declarations both answer to resolves to neither and stays the text
the author wrote: sending a reader to the wrong declaration is worse than not
linking.

Highlighting reuses the website's own grammar module rather than a copy that
would drift. A page's JavaScript is one classic script, where a module keyword is
a syntax error that would take search and navigation down with highlighting, so
the export checks that module's shape and fails, naming the file, when it holds
anything but the single expected `export const ospreyGrammar`. Because that file
is compiled into the binary, continuous integration classifies it as compiler
code, not website content.

Search does not fetch. The index is loaded as a script, because a page opened
directly from the filesystem has an opaque origin where fetching a sibling file
fails as a cross-origin request — with nothing on the page to say why.

### Additional pages `[DOC-EXPORT-PAGES]`

Repeat `--docs-page <file.md-or-directory>` to include authored Markdown. A
directory is scanned recursively for Markdown pages, preserving its relative
structure under `guides/`. A page's first level-one heading supplies its title.
Additional pages appear in navigation and search alongside generated APIs.
Missing inputs and conflicting page paths are errors.

### Themes and custom CSS `[DOC-EXPORT-CSS]`

HTML supports three templates through `--docs-theme osprey|midnight|paper`.
`osprey` uses a blue reference layout with module cards and an article outline;
`midnight` uses dark surfaces, compact spacing and monospaced details; `paper`
uses editorial serif headings and ruled lists. Typography and component layouts
vary between templates. They share semantic markup, responsive navigation and
CSS custom properties, so customization does not require replacing the renderer.

Every template holds the article to a readable measure rather than stretching
prose across a wide monitor, sets body text no smaller than the 16px browser
default, and draws the heading outline as a bounded panel in the right margin —
a rule, a border or a surface of its own — so it reads as a list of links
beside the article rather than a second column of prose. The outline is hidden
below 1180px, where there is no margin for it to sit in.

The landing page promotes project modules and authored guides before the full
reference. Navigation groups are keyboard-operable disclosures. JavaScript folds
inactive groups, adds links to article headings, highlights Osprey code without
changing its text, and provides the `/` search shortcut. Without JavaScript every
navigation group remains open and all content stays readable. Highlighting reuses
the website's language grammar and loads no network assets.

Repeat `--docs-css <file.css>` to copy custom stylesheets into the output site.
They are linked after the selected theme, in argument order. Stylesheets may
override theme properties and ordinary selectors. CSS and nondefault themes
require HTML output. Unknown themes, formats and options, and missing argument
values fail with a usage error instead of silently generating a different result.

## Implementation and verification

The implementation is shared by the compiler, editor, CLI exporter and doctest runner. [The usage guide](../../../website/src/docs/documentation.md) includes Default and ML examples extracted from the rendered page and executed by the browser suite.

From the repository root:

```sh
cargo test -p osprey-cli --test cli_e2e api_docs
cargo test -p osprey-cli --test cli_e2e doctests
make docs-html-test
```

The CLI tests check source validation, public API visibility, inferred signatures, authored pages and doctest execution. Renderer unit tests live under `crates/osprey-cli/src/docs/html/tests*`. The corpus harness separately runs doctests under native memory backends and WebAssembly, rejecting output drift, malformed summaries and ARC leaks.

`make docs-html-test` builds the compiler and runs [the standalone Chromium checks](../../../scripts/verify-docs-html.mjs). They cover generated sites opened over `file://`, all three themes, custom CSS ordering, search, links, mobile/keyboard navigation and inert Markdown.

For the [website's generated-documentation tests](../../../website/tests/api-docs.spec.js), run `cd website && npm test -- tests/api-docs.spec.js`. They additionally exercise HTTP hosting under a URL prefix, executable guide examples, responsive layouts and navigation without JavaScript.

`node scripts/preview-docs.mjs` generates all three themed sites and a comparison gallery with desktop and phone screenshots. Passing documentation tests does not establish that unrelated compiler, editor or application checks pass; use the current CI results for that status.
