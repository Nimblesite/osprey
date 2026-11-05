# @osprey/jinja2

An Osprey Python-flavor port of [jinja2](https://github.com/pallets/jinja) 3.1.6: lexing, parsing and rendering of the Jinja2 chat-template subset a model's `chat_template` uses.

Values use the port's typed representation. This is a template subset, not the complete Python object model or extension ecosystem.

The [Python-flavor ports contract](../../../../docs/specs/ecosystem/0043-PythonFlavorPorts.md) defines [PORT-SUITE], [PORT-TEST-ORACLE] and [PORT-SCOPE]. The assertions in `tests/main.py` compare rendering with the values the real CPython package computed in `oracle/jinja2.cpython`, which writes `tests/expected.py`. `src/` is Osprey source, not CPython; run it with the Osprey compiler.

```sh
target/release/osprey src/osprey/packages/jinja2/tests --run
target/release/osprey src/osprey/packages/jinja2/tests --run --memory=gc
OSPREY_ARC_DEBUG=1 target/release/osprey src/osprey/packages/jinja2/tests --run --memory=arc
```

The Rust integration harness discovers `tests/osprey.toml`, compares stdout with `tests/expected_output.txt` under all three memory backends, and requires ARC to exit without live objects. Presence of a suite does not establish that it passes; compiler diagnostics, assertion failures and missing golden files remain failures.

## Origin, license and thanks

**Original project:** [Jinja](https://github.com/pallets/jinja).

**How it was ported.** Osprey's Python flavor reads like Python, so this port keeps Jinja's module, function and argument names wherever it can, and its tests compare results with Jinja 3.1.6 running under CPython. That keeps it closer to the original than a port to a different-looking language usually is. It is still a reimplementation: the code is written for Osprey's types, effects and runtime, and Jinja is not bundled, linked or needed at run time.

**License.** Jinja is BSD-3-Clause licensed. This package is MIT, Copyright NIMBLESITE PTY LTD, Australia; that grant covers Osprey's own code and does not relicense anything of Jinja's. [NOTICE.md](NOTICE.md) lists any third-party material the package carries and the terms that come with it (copies in [LICENSES/](LICENSES/)).

**Thanks** to the Pallets team and the Jinja contributors for [Jinja](https://github.com/pallets/jinja), and to the Qwen team at Alibaba Cloud, whose Apache-2.0 chat template is a test fixture here. Naming Jinja credits it; it does not imply that its maintainers endorse this port.

## What the port covers

| Osprey module | Jinja2 surface it reimplements |
| --- | --- |
| `Lexer` | `jinja2.lexer`: `{{ }}`, `{% %}`, `{# #}`, the `-`/`+` whitespace-control signs, `trim_blocks`, `lstrip_blocks`, closing-character search that skips string literals and nested brackets |
| `Parser` | `jinja2.parser`: `if`/`elif`/`else`, `for … in …` with an `if` filter, `k, v` unpacking and `else`, `set` and namespace-attribute `set`, `macro` with defaults, `endmacro` |
| `Expr` | `jinja2.nodes` evaluation: literals, parenthesised tuples, list and dict literals, comparisons and `in`, boolean operators, `+ - * / // % ~`, unary signs, conditional expressions, subscripts and slices, attribute and method access, filters, tests, calls |
| `Value` | `jinja2.runtime.Undefined` semantics, Python truthiness, equality and ordering, `len`, repr, slices, `tojson`, scope frames, the macro table |
| `Filters` | `trim`, `upper`, `lower`, `capitalize`, `length`/`count`, `tojson`, `join`, `replace`, `default`/`d`, `first`, `last`, `list`, `string`, `safe`, `items`, `map`, `select`/`reject`/`selectattr`/`rejectattr`; tests `defined`, `undefined`, `none`, `string`, `number`, `integer`, `float`, `boolean`, `true`, `false`, `equalto`/`eq`/`==`, `ne`/`!=`, `in`, `mapping`, `iterable`, `sequence`, `sameas`; string and dict methods; globals `raise_exception`, `namespace`, `dict`, `range` |
| `Render` | `jinja2.environment.Template.render`: the node walk, scope threading, loop objects, macro expansion, whitespace assembly |

`Render.chat_environment()` is `Environment(trim_blocks=True, lstrip_blocks=True)`, the whitespace configuration transformers' chat templates are compiled with.

## The Qwen3.8 chat template

`tests/fixtures/qwen3.8-27b.chat_template.jinja` is the unmodified template of [Qwen/Qwen3.8-27B](https://huggingface.co/Qwen/Qwen3.8-27B/tree/1d4bf0f2ff6012fd82039f2fa52739d0dd7c60c0) (revision `1d4bf0f2ff6012fd82039f2fa52739d0dd7c60c0`); see [NOTICE.md](NOTICE.md) and [`LICENSES/`](LICENSES). The oracle renders it through `transformers.utils.chat_template_utils.render_jinja_template` — the render step `apply_chat_template` calls, with transformers' own `tojson`, `raise_exception` and sandboxed environment — and the suite requires the port to reproduce every rendering byte for byte, and every rejection with the same message. Cases cover messages, content blocks, tools, tool calls and multi-turn tool results, thinking (`enable_thinking`), `reasoning_effort`, `preserve_thinking`, `add_generation_prompt` on and off, vision markers, unicode and a message far longer than the template.

## Divergences

Each divergence is pinned by an assertion in `tests/main.py`, so the suite states the port's own behavior where it cannot match CPython.

- **A namespace write inside a macro does not escape the call.** Values are immutable, so `{% set ns.attr = … %}` inside a macro body rebinds the argument's own frame instead of the caller's namespace object. CPython mutates the shared namespace. The Qwen3.8 template's vision markers still render byte for byte identically, because it reads that counter only for `add_vision_id`; with numbering on, each message counts from 1 where CPython continues across messages.
- **A parenthesised tuple is a list.** Membership and iteration match; the printed form and equality against a list do not (`('a', 'b')` prints `['a', 'b']`).
- **`|safe` is the identity.** The port has no autoescaping, which is also Jinja2's default for a plain `Environment`, so marking a value safe changes nothing.
- **`strftime_now` is unavailable.** It depends on the host clock, which the port does not read; calling it is an error.
- **`{% break %}`, `{% continue %}`, `{% include %}`, `{% import %}`, `{% extends %}`, `{% block %}` and filter/test registration are out of scope.** An unknown statement, filter or test is an error, never a silently empty rendering.

## Resource bounds

Nothing in the port recurses once per character, byte or list element: character scans, splits, slices, membership, equality, `dict` updates, the lexer's tag scans and the expression tokenizer's string-literal scan all fold over `range`. Stack use therefore follows the template's structure, never the length of the template text or of a message. Measured locally: a 600 KB string literal, a 40 KB template with 2,000 tags, a 900 KB multibyte literal and a 320 KB text body all render.
