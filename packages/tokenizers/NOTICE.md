# @osprey/tokenizers: licensing and attribution

Original Osprey contributions are licensed under the MIT license in [LICENSE](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia. The pre-existing Christian Findlay copyright is retained there. This grant covers our original contributions; it does not replace third-party ownership or terms.

The cumulative package license expression is `MIT AND Apache-2.0 AND Unicode-DFS-2016 AND Unicode-3.0`. `AND` requires compliance with every applicable license; it is not a choice of licenses. Upstream names identify provenance and do not imply endorsement.

These files are Osprey adaptations of the components below. Changes include translation into Osprey's Python flavor, immutable data structures, explicit handlers where used, and the documented API subset. Generated character tables are derived data. Preserve this change summary, the license texts and the applicable copyright notices in source distributions and in the documentation/materials supplied with compiled distributions.

## tokenizers

Upstream: [huggingface/tokenizers](https://github.com/huggingface/tokenizers). Reference: 0.23.3. Retained text: [LICENSES/tokenizers.txt](LICENSES/tokenizers.txt). [License source](https://files.pythonhosted.org/packages/e0/7c/2cabb2174e772636683008f2c5621949b645da7d303c596589e84516a184/tokenizers-0.23.3.tar.gz).

## Unicode-DFS-2016

Upstream: [unicode-org/icu/tree/release-73-2](https://github.com/unicode-org/icu/tree/release-73-2). Reference: upstream audit snapshot. Retained text: [LICENSES/Unicode-DFS-2016.txt](LICENSES/Unicode-DFS-2016.txt). [License source](https://github.com/unicode-org/icu/blob/release-73-2/icu4c/LICENSE).

## Unicode data versions

The bundled [Unicode License v3](LICENSES/Unicode-3.0.txt) is retained alongside the older Unicode data license. Oracle labels can report CPython's Unicode version independently of the regex package's own tables; the labels alone do not establish that both use the same Unicode release. Preserve both notices.

## Distribution

The package publisher includes `LICENSE`, `NOTICE.md` and the full `LICENSES/` tree in the digest-pinned release. Fetch/install retain those files. Applications distributing compiled code must also ship the applicable license and notice files; compilation does not carry these documents into an executable automatically.
