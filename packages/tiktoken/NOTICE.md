# @osprey/tiktoken: licensing and attribution

Original Osprey contributions are licensed under the MIT license in [LICENSE](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia. The pre-existing Christian Findlay copyright is retained there. This grant covers our original contributions; it does not replace third-party ownership or terms.

The cumulative package license expression is `MIT AND Apache-2.0 AND CNRI-Python AND Unicode-DFS-2016 AND Unicode-3.0`. `AND` requires compliance with every applicable license; it is not a choice of licenses. Upstream names identify provenance and do not imply endorsement.

These files are Osprey adaptations of the components below. Changes include translation into Osprey's Python flavor, immutable data structures, explicit handlers where used, and the documented API subset. Generated character tables are derived data. Preserve this change summary, the license texts and the applicable copyright notices in source distributions and in the documentation/materials supplied with compiled distributions.

## tiktoken

Upstream: [openai/tiktoken](https://github.com/openai/tiktoken). Reference: 0.14.0. Retained text: [LICENSES/tiktoken.txt](LICENSES/tiktoken.txt). [License source](https://files.pythonhosted.org/packages/66/62/167a842aa0429d45f5e797354fd4343a96f6043d67d0513c675c7b8d36e6/tiktoken-0.14.0.tar.gz).

## regex

Upstream: [mrabarnett/mrab-regex](https://github.com/mrabarnett/mrab-regex). Reference: 2026.9.29. Retained text: [LICENSES/regex.txt](LICENSES/regex.txt). [License source](https://files.pythonhosted.org/packages/fc/f2/af1da9d3ceed77bfcdce40427d49ba0be94e4fe84245e3bfef68c10e75b6/regex-2026.9.29.tar.gz).

## CPython

Upstream: [python/cpython](https://github.com/python/cpython). Reference: 3.12.13. Retained text: [LICENSES/CPython.txt](LICENSES/CPython.txt). [License source](https://github.com/python/cpython/blob/v3.12.13/LICENSE).

## Unicode-DFS-2016

Upstream: [unicode-org/icu/tree/release-73-2](https://github.com/unicode-org/icu/tree/release-73-2). Reference: upstream audit snapshot. Retained text: [LICENSES/Unicode-DFS-2016.txt](LICENSES/Unicode-DFS-2016.txt). [License source](https://github.com/unicode-org/icu/blob/release-73-2/icu4c/LICENSE).

## gpt2

Upstream: [openai/gpt-2](https://github.com/openai/gpt-2). Reference: upstream audit snapshot. Retained text: [LICENSES/gpt2.txt](LICENSES/gpt2.txt). [License source](https://github.com/openai/gpt-2/blob/master/LICENSE).

Secret Labs' SRE copyright and the CNRI Python 1.6 terms are retained through the complete CPython license. This is a modified Osprey implementation, not CPython.

The regex package's license includes Apache-2.0 additions and the original CNRI-Python/SRE lineage. Unicode data terms are retained separately. GPT-2 vocabulary subsets in the test fixtures originate in OpenAI's GPT-2 data and carry its modified MIT notice; that notice is retained as attribution to test data and does not change the license of unrelated port source. The cl100k fixture records reference outputs and a truncated rank table from tiktoken's publicly downloaded data. The [upstream collaborator's clarification](https://github.com/openai/tiktoken/issues/92#issuecomment-1497875652) explicitly applies tiktoken's MIT license to those encoding files; a [local copy](LICENSES/tiktoken-data-permission.txt) travels with this package. Neither complete pretrained vocabularies nor model weights are included in package releases; any future model/data redistribution needs its own license review.

## Unicode data versions

The bundled [Unicode License v3](LICENSES/Unicode-3.0.txt) is retained alongside the older Unicode data license. Oracle labels can report CPython's Unicode version independently of the regex package's own tables; the labels alone do not establish that both use the same Unicode release. Preserve both notices.

## Distribution

The package publisher includes `LICENSE`, `NOTICE.md` and the full `LICENSES/` tree in the digest-pinned release. Fetch/install retain those files. Applications distributing compiled code must also ship the applicable license and notice files; compilation does not carry these documents into an executable automatically.
