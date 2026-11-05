# @osprey/tokenizers: licensing and upstream acknowledgement

The Osprey implementation is provided under the normal [MIT license](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia, retaining the earlier Christian Findlay notice. This does not claim ownership of any third-party material identified below.

## Upstream API and behavior references

These repositories and releases are acknowledged as API, behavior and test-oracle references. Their licenses are not assigned to this implementation merely because the API is reimplemented. The port does not bundle or link the upstream Python package.

- [tokenizers](https://github.com/huggingface/tokenizers): 0.23.3.

## Specifically retained third-party material

`bytes_to_unicode` in `oracle/tokenizers.cpython`, the CPython script that computes this package's test values, is adapted from [`src/encoder.py`](https://github.com/openai/gpt-2/blob/master/src/encoder.py) in OpenAI's GPT-2, Software Copyright (c) 2019 OpenAI, under the modified MIT license in [`LICENSES/gpt-2-Modified-MIT.txt`](LICENSES/gpt-2-Modified-MIT.txt). It is used only by that oracle.

The cumulative expression for the identified material is `MIT AND Unicode-DFS-2016 AND Unicode-3.0`. These additional terms apply to that material; they do not transfer ownership of the Osprey implementation or turn upstream API references into dependencies.

### unicode15

Material: Derived Unicode 15.0.0 character-property tables, checked against the official UCD. Locations: `src/Unicode.py`.
Original source: [https://github.com/unicode-org/icu/tree/release-73-2](https://github.com/unicode-org/icu/blob/release-73-2/icu4c/LICENSE). Retained required text: [LICENSES/Unicode-DFS-2016.txt](LICENSES/Unicode-DFS-2016.txt).

### unicode-tokenizers

Material: Unicode canonical decomposition, composition, combining-class and general-category tables, recovered from the output of tokenizers 0.23.3 by the oracle. The Unicode data version they encode was not recorded. Locations: `src/NfcData.py`, `src/Category.py`.
Original source: [https://github.com/unicode-org/icu](https://github.com/unicode-org/icu/blob/main/LICENSE). Retained required text: [LICENSES/Unicode-3.0.txt](LICENSES/Unicode-3.0.txt).

The Unicode notices cover derived property/case tables, not the regex or tokenizer engine. The table data matches Unicode 15.0.0; only the corresponding data license is retained. CPython's separate Unicode version does not determine the regex package's data version.

## Distribution

The publisher carries `LICENSE`, this acknowledgement and any required `LICENSES/` files in the digest-pinned release. Source or compiled redistributions containing the specifically retained material must carry its applicable notices. Upstream project names provide acknowledgement and imply no endorsement.
