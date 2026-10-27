# @osprey/re: licensing and upstream acknowledgement

The Osprey implementation is provided under the normal [MIT license](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia, retaining the earlier Christian Findlay notice. This does not claim ownership of any third-party material identified below.

## Upstream API and behavior references

These repositories and releases are acknowledged as API, behavior and test-oracle references. Their licenses are not assigned to this implementation merely because the API is reimplemented. The port does not bundle or link the upstream Python package.

- [re](https://github.com/python/cpython): 3.12.13.

## Specifically retained third-party material

The cumulative expression for the identified material is `MIT AND Unicode-DFS-2016`. These additional terms apply to that material; they do not transfer ownership of the Osprey implementation or turn upstream API references into dependencies.

### unicode15

Material: Derived Unicode 15.0.0 character-property tables, checked against the official UCD. Locations: `src/tables.py`.
Original source: [https://github.com/unicode-org/icu/tree/release-73-2](https://github.com/unicode-org/icu/blob/release-73-2/icu4c/LICENSE). Retained required text: [LICENSES/Unicode-DFS-2016.txt](LICENSES/Unicode-DFS-2016.txt).

The Unicode notices cover derived property/case tables, not the regex or tokenizer engine. The table data matches Unicode 15.0.0; only the corresponding data license is retained. CPython's separate Unicode version does not determine the regex package's data version.

## Distribution

The publisher carries `LICENSE`, this acknowledgement and any required `LICENSES/` files in the digest-pinned release. Source or compiled redistributions containing the specifically retained material must carry its applicable notices. Upstream project names provide acknowledgement and imply no endorsement.
