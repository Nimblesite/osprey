# Packages

All package infrastructure and libraries live here. [manager/](manager/README.md) contains the package client, registry server, resolver and their tests. The sibling library directories use the same manifests, publishing, installation and coverage rules across Default, ML and Python flavor. Porting a Python project's API does not create a separate kind of package.

## Licensing and provenance audit

Reviewed on 25 October 2026. Each package carries the normal MIT grant from **NIMBLESITE PTY LTD, Australia**, retaining the earlier Christian Findlay notice. The packages reimplement selected APIs and behavior in Osprey; they do not bundle, link or redistribute the corresponding Python packages. An API reference, oracle output or the word “port” does not by itself import an upstream license.

Each package's `NOTICE.md` acknowledges the original repositories and comparison releases. Upstream license copies are retained only for the specific reused implementation or derived data identified below. The original authors retain ownership of that material; the Nimblesite MIT grant does not reassign it.

| Package | Original API/behavior reference | Specifically retained third-party material |
| --- | --- | --- |
| [einops](einops/NOTICE.md) | Alex Rogozhnikov, einops 0.8.2 | None identified; MIT Osprey implementation |
| [faiss](faiss/NOTICE.md) | Meta/Facebook, Faiss 1.15.1 | None identified; MIT Osprey implementation |
| [flash_attn](flash_attn/NOTICE.md) | Dao-AILab FlashAttention conventions | None identified; MIT Osprey implementation |
| [jinja2](jinja2/NOTICE.md) | Pallets Jinja 3.1.6 | None identified; MIT Osprey implementation |
| [micrograd](micrograd/NOTICE.md) | Andrej Karpathy, micrograd 0.1.0 and nn-zero-to-hero | None identified; MIT Osprey implementation |
| [native](native/NOTICE.md) | Original Osprey framework | MIT |
| [numpy](numpy/NOTICE.md) | NumPy 2.5.4 | None identified; MIT Osprey implementation |
| [re](re/NOTICE.md) | CPython 3.12.13 re behavior | Derived Unicode property/case tables in `src/tables.py` |
| [rouge_score](rouge_score/NOTICE.md) | Google rouge-score 0.1.2 and NLTK Porter behavior | None identified; MIT Osprey implementation |
| [sacrebleu](sacrebleu/NOTICE.md) | SacreBLEU 2.6.0 | None identified; MIT Osprey implementation |
| [safetensors](safetensors/NOTICE.md) | Safetensors 0.8.0 format and behavior | None identified; MIT Osprey implementation |
| [scipy](scipy/NOTICE.md) | SciPy special-function behavior | Adapted Cephes/xsf numerical kernels and copied coefficients; Boost-derived digamma approximation |
| [sklearn](sklearn/NOTICE.md) | scikit-learn 1.9.1 | None identified; MIT Osprey implementation |
| [testing](testing/NOTICE.md) | NumPy closeness-comparison formula | None identified; MIT Osprey assertions |
| [tiktoken](tiktoken/NOTICE.md) | OpenAI tiktoken 0.14.0 | Derived Unicode tables; copied encoding-rank subsets in test fixtures |
| [tokenizers](tokenizers/NOTICE.md) | Hugging Face tokenizers 0.23.3 | Derived Unicode tables in `src/Unicode.py` |
| [torch](torch/NOTICE.md) | PyTorch 2.14.1 | None identified; MIT Osprey implementation |
| [transformers](transformers/NOTICE.md) | Transformers 5.20.2 generation behavior | None identified; MIT Osprey implementation |

## Why the remaining license texts are present

SciPy's numerical implementation explicitly identifies its Cephes/xsf origin. The coefficient arrays match those kernels, and the digamma approximation contains John Maddock's Boost coefficients. The retained Cephes/xsf BSD and Boost licenses apply to those identified files. They are not assigned merely because the public API resembles SciPy.

The regex/tokenizer Unicode tables are generated from character data. Their data notices apply to those tables, not to the independently expressed matching or tokenization engines. The `re` and `tokenizers` property tables match Unicode 15.0.0; the `tiktoken` tables match the regex package's Unicode 18.0.0 data within their documented coverage. Only the corresponding data license is retained for each package. CPython's `unicodedata` version does not identify regex's Unicode version.

Tiktoken test fixtures contain actual encoding-rank subsets, unlike ordinary expected outputs computed by an oracle. The [upstream collaborator explicitly confirms that tiktoken's MIT license covers those encoding files](https://github.com/openai/tiktoken/issues/92#issuecomment-1497875652). Its required MIT text is retained for the copied data; the clarification is cited as evidence and imposes no additional bundled-document requirement. No full tokenizer package, model weights or complete pretrained vocabulary is bundled.

No BSD/Apache/Python license is assigned solely because NumPy, PyTorch, Jinja, CPython, Hugging Face or another package supplied an API or test oracle. Ordinary output from a program is not automatically a copy of its implementation. Future source or data incorporation needs its own review.

## Evidence and limits

[source-audit.json](source-audit.json) records 80 reference comparisons, including 42 files from the exact source distributions named by the oracles. The lexical comparison found short einops compatibility strings/constants and SciPy coefficient sequences. Token matches are candidates for inspection, not proof that a port is a derivative work. The einops matches do not identify a substantive shared implementation block. This audit does not assert clean-room authorship from the absence of matches.

The exact historical commit used while writing every implementation was not recorded. Comparison versions are reference versions, not asserted derivation commits. This bounded technical review is not a legal opinion or a guarantee against unidentified copying. No identified license prohibits the specific reuse retained here when its stated conditions are met.

A second review on 25 October 2026 freshly retrieved and verified all 80 reference-file hashes and all seven retained third-party license texts. It compared 140 current Python-flavor implementation and fixture files, checked shared literal strings separately, and repeated eight Unicode classification-table comparisons with zero differences. Additional safetensors byte-sequence matches are ordinary serializer output generated by `scripts/oracles/safetensors.py`; no new reused implementation or licensed dataset was identified. Six tokenizers source-archive paths in the evidence record were corrected without changing their verified content hashes or license assignments. The license guard, its three regressions, and the focused publication and fetch cases pass.

[license-audit.json](license-audit.json) separates informational `references` from specifically retained `upstreams`. Every retained item names the actual files, evidence basis, source and SHA-256 of its required notice. Whole upstream NOTICE collections and unrelated bundled dependency licenses are not included. Identical standard text within a package is stored once.

## Package distribution and checks

The publisher retains root `LICENSE`, `COPYING`, `NOTICE`, `AUTHORS` and `THIRD_PARTY_NOTICES` files, including `.txt`/`.md` forms, and a `LICENSES/` directory when one is present. It does not infer upstream dependencies or add license texts from a package's name. Legal files participate in `SourceDigest`, survive fetch/install, and tampering invalidates a bundle. Applications distributing compiled code that contains the specifically identified reused material must supply its applicable documents; compiling does not embed them automatically.

Run `node scripts/verify-package-licenses.mjs` from the repository root. It checks Nimblesite's MIT grant, the reviewed metadata, evidence for retained material and only the modification/data notices required for that material. It rejects unreviewed license files. This also runs under `make lint` and `make ci`.

Regression cases in [publish_tests.osp](manager/cli/tests/src/publish_tests.osp) and [fetch_tests.osp](manager/cli/tests/src/fetch_tests.osp) check legal-file retention and rejection of altered attribution. Filters are exact test names; each command must execute one passing case:

```sh
target/release/osprey test packages/manager/cli/tests --filter "publish: licenses survive publication"
target/release/osprey test packages/manager/cli/tests --filter "fetch: licenses and attribution are preserved"
```

`node --test scripts/test-package-licenses.mjs` checks the reviewed bundles, rejection of missing required original attribution, and rejection of unnecessary upstream license copies; expect three passing tests.
