# @osprey/tiktoken: licensing and upstream acknowledgement

The Osprey implementation is provided under the normal [MIT license](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia, retaining the earlier Christian Findlay notice. This does not claim ownership of any third-party material identified below.

## Upstream API and behavior references

These repositories and releases are acknowledged as API, behavior and test-oracle references. Their licenses are not assigned to this implementation merely because the API is reimplemented. The port does not bundle or link the upstream Python package.

- [tiktoken](https://github.com/openai/tiktoken): 0.14.0.
- [regex](https://github.com/mrabarnett/mrab-regex): 2026.9.29.
- [re](https://github.com/python/cpython): 3.12.13.

## Specifically retained third-party material

The cumulative expression for the identified material is `MIT AND Unicode-3.0`. These additional terms apply to that material; they do not transfer ownership of the Osprey implementation or turn upstream API references into dependencies.

### tiktoken

Material: Copied encoding-rank subsets and upstream explicit MIT permission. Locations: `tests/expected.py`.
Original source: [https://github.com/openai/tiktoken](https://files.pythonhosted.org/packages/66/62/167a842aa0429d45f5e797354fd4343a96f6043d67d0513c675c7b8d36e6/tiktoken-0.14.0.tar.gz). Retained required text: [LICENSES/tiktoken.txt](LICENSES/tiktoken.txt).

### unicode3

Material: Derived Unicode 18.0.0 character-property tables, checked against the official UCD. Locations: `src/tables.py`.
Original source: [https://github.com/unicode-org/icu](https://github.com/unicode-org/icu/blob/main/LICENSE). Retained required text: [LICENSES/Unicode-3.0.txt](LICENSES/Unicode-3.0.txt).

### tiktoken-encoding-permission

Material: Copied encoding-rank subsets and upstream explicit MIT permission. Locations: `tests/expected.py`.
Original source: [https://github.com/openai/tiktoken](https://github.com/openai/tiktoken/issues/92#issuecomment-1497875652). Retained required text: [LICENSES/tiktoken-data-permission.txt](LICENSES/tiktoken-data-permission.txt).

The copied GPT-2/cl100k encoding-rank subsets are test data. OpenAI's [explicit clarification](https://github.com/openai/tiktoken/issues/92#issuecomment-1497875652) applies its MIT license to encoding files. The complete upstream tokenizer package and vocabularies are not included.

The Unicode notices cover derived property/case tables, not the regex or tokenizer engine. The table data matches Unicode 18.0.0; only the corresponding data license is retained. CPython's separate Unicode version does not determine the regex package's data version.

## Distribution

The publisher carries `LICENSE`, this acknowledgement and any required `LICENSES/` files in the digest-pinned release. Source or compiled redistributions containing the specifically retained material must carry its applicable notices. Upstream project names provide acknowledgement and imply no endorsement.
