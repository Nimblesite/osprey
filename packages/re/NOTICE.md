# @osprey/re: licensing and attribution

Original Osprey contributions are licensed under the MIT license in [LICENSE](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia. The pre-existing Christian Findlay copyright is retained there. This grant covers our original contributions; it does not replace third-party ownership or terms.

The cumulative package license expression is `MIT AND Python-2.0 AND CNRI-Python AND Unicode-DFS-2016 AND Unicode-3.0`. `AND` requires compliance with every applicable license; it is not a choice of licenses. Upstream names identify provenance and do not imply endorsement.

These files are Osprey adaptations of the components below. Changes include translation into Osprey's Python flavor, immutable data structures, explicit handlers where used, and the documented API subset. Generated character tables are derived data. Preserve this change summary, the license texts and the applicable copyright notices in source distributions and in the documentation/materials supplied with compiled distributions.

## CPython

Upstream: [python/cpython](https://github.com/python/cpython). Reference: 3.12.13. Retained text: [LICENSES/CPython.txt](LICENSES/CPython.txt). [License source](https://github.com/python/cpython/blob/v3.12.13/LICENSE).

## Unicode-DFS-2016

Upstream: [unicode-org/icu/tree/release-73-2](https://github.com/unicode-org/icu/tree/release-73-2). Reference: upstream audit snapshot. Retained text: [LICENSES/Unicode-DFS-2016.txt](LICENSES/Unicode-DFS-2016.txt). [License source](https://github.com/unicode-org/icu/blob/release-73-2/icu4c/LICENSE).

## Retained source attribution

- Copyright (c) 1997-2001 by Secret Labs AB.  All rights reserved.
- Copyright (c) 1998-2001 by Secret Labs AB.  All rights reserved.

Secret Labs' SRE copyright and the CNRI Python 1.6 terms are retained through the complete CPython license. This is a modified Osprey implementation, not CPython.

## Unicode data versions

The bundled [Unicode License v3](LICENSES/Unicode-3.0.txt) is retained alongside the older Unicode data license. Oracle labels can report CPython's Unicode version independently of the regex package's own tables; the labels alone do not establish that both use the same Unicode release. Preserve both notices.

## Distribution

The package publisher includes `LICENSE`, `NOTICE.md` and the full `LICENSES/` tree in the digest-pinned release. Fetch/install retain those files. Applications distributing compiled code must also ship the applicable license and notice files; compilation does not carry these documents into an executable automatically.
