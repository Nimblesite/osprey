# @osprey/pydantic-ai: licensing and upstream acknowledgement

The Osprey implementation is provided under the normal [MIT license](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia. This does not claim ownership of any third-party material identified below.

## What this package is

An Osprey Python-flavor port of the pydantic-ai 2.46.0 agent engine: enough of the library to run a tool-calling agent against any OpenAI-compatible chat API. The port keeps upstream's module and function names, message and event shapes, Chat Completions request and response mapping, `TestModel` schema-data rules and exception texts, so an Osprey program reads like the CPython one. The Osprey source is a reimplementation in the Python flavor, not a copy; the upstream Python package is not bundled, linked or redistributed.

## The original work and its authors

This port exists because of the original project. It is not endorsed by Pydantic Services Inc. or by the authors named below.

- Project: [pydantic-ai](https://github.com/pydantic/pydantic-ai) 2.46.0 (tag [`v2.46.0`](https://github.com/pydantic/pydantic-ai/blob/v2.46.0/pydantic_ai_slim/pydantic_ai/__init__.py)), by **Pydantic Services Inc.** and contributors.
- Copyright line, exactly as the upstream licence states it: *Copyright (c) Pydantic Services Inc. 2024 to present.*
- Authors named in the 2.46.0 package metadata: Douwe Maan, David Sanchez, Aditya Vardhan, David Montague, Marcelo Trylesinski, Alex Hall, Samuel Colvin, and the project's other contributors.
- Upstream licence (MIT), as a pointer to the exact compared release — not copied here: [github.com/pydantic/pydantic-ai/blob/v2.46.0/LICENSE](https://github.com/pydantic/pydantic-ai/blob/v2.46.0/LICENSE).
- [openai-python](https://github.com/openai/openai-python): 3.16.2, the Chat Completions wire format the oracle records.

## License scope

No additional upstream software license is imposed based solely on the API name, behavior, output of an oracle, short compatibility diagnostics or the label “port”. No substantive shared implementation block was identified in the recorded comparison. The full-wheel comparison recorded in [source-audit.json](../source-audit.json) (15 local files against all 332 upstream modules) found zero matching 20-token windows, a longest shared normalised line run of two trivial pattern-match lines, and, outside the deliberately kept exception texts and configuration key identifiers, no shared string literal of 12 or more characters. A matching API does not establish either copying or independent authorship; review any future incorporation of upstream source or data separately.

## Distribution

The publisher carries `LICENSE`, this acknowledgement and any required `LICENSES/` files in the digest-pinned release. Source or compiled redistributions containing the specifically retained material must carry its applicable notices. Upstream project names provide acknowledgement and imply no endorsement.
