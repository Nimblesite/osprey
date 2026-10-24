# Package license audit

Audited on 25 October 2026. Every package carries the normal MIT grant for original Osprey contributions, **Copyright (c) 2026 NIMBLESITE PTY LTD, Australia**, retaining the earlier Christian Findlay notice. Each package's `NOTICE.md` identifies the original repositories and authors, adaptations and additional terms. Upstream ownership and conditions remain intact.

| Package | Upstream/reference | Terms retained alongside our MIT grant |
| --- | --- | --- |
| [einops](einops/NOTICE.md) | Alex Rogozhnikov, einops 0.8.2 | Upstream MIT |
| [faiss](faiss/NOTICE.md) | Meta/Facebook, Faiss 1.15.1 | Upstream MIT |
| [flash_attn](flash_attn/NOTICE.md) | Dao-AILab FlashAttention, documented 2.1+ semantics | BSD-3-Clause, AUTHORS |
| [jinja2](jinja2/NOTICE.md) | Pallets Jinja 3.1.6 | BSD-3-Clause |
| [micrograd](micrograd/NOTICE.md) | Andrej Karpathy, micrograd 0.1.0 and nn-zero-to-hero | Both upstream MIT copyrights |
| [native](native/NOTICE.md) | Original Osprey framework | MIT; no third-party port identified |
| [numpy](numpy/NOTICE.md) | NumPy 2.5.4 | BSD-3-Clause |
| [re](re/NOTICE.md) | CPython 3.12.13, Secret Labs SRE, Unicode | Complete Python license history, CNRI-Python, Unicode terms |
| [rouge_score](rouge_score/NOTICE.md) | Google rouge-score 0.1.2, NLTK 3.10.4 Porter stemmer | Apache-2.0, both projects' attribution |
| [sacrebleu](sacrebleu/NOTICE.md) | Matt Post and SacreBLEU contributors, 2.6.0 | Apache-2.0 |
| [safetensors](safetensors/NOTICE.md) | Hugging Face / safetensors contributors, 0.8.0 | Apache-2.0 |
| [scipy](scipy/NOTICE.md) | SciPy 1.18.1, xsf, Stephen L. Moshier's Cephes, John Maddock's digamma approximation | BSD-3-Clause, Boost Software License 1.0 |
| [sklearn](sklearn/NOTICE.md) | scikit-learn 1.9.1 | BSD-3-Clause |
| [testing](testing/NOTICE.md) | Original Osprey assertions; NumPy comparison semantics | NumPy BSD-3-Clause retained conservatively |
| [tiktoken](tiktoken/NOTICE.md) | OpenAI tiktoken 0.14.0, regex 2026.9.29, SRE, Unicode | MIT, Apache-2.0, CNRI-Python, Unicode; historical GPT-2 notice for test data |
| [tokenizers](tokenizers/NOTICE.md) | Hugging Face tokenizers 0.23.3, Unicode | Apache-2.0, Unicode terms |
| [torch](torch/NOTICE.md) | PyTorch 2.14.1 | BSD-3-Clause, complete upstream NOTICE |
| [transformers](transformers/NOTICE.md) | Hugging Face Transformers 5.20.2 generation utilities | Apache-2.0; relevant Hugging Face, Google/DeepMind, Facebook and NVIDIA notices |

Manifest expressions use `MIT AND ...`, requiring applicable terms together. They offer no MIT-only alternative for upstream material. A CPython oracle using a wheel does not mean its CUDA, BLAS or other binary dependencies are shipped by these source ports. Re-audit future vendoring, datasets, model weights and native binaries.

## Corrections and evidence

All 18 manifests previously declared MIT alone, and none of these package directories contained an upstream license bundle. The audit added standalone MIT licenses, original repository acknowledgements, full applicable upstream terms, source modification notices, cumulative SPDX expressions and automatic publication of legal files. The `re` and `tiktoken` table generators preserve the modification notice when regenerated.

SciPy's Cephes coefficients and Boost-derived digamma approximation retain their original authors and conditions. Generated Unicode tables retain both the older Unicode data permission and Unicode License v3: a Python `unicodedata` version label does not establish the separate regex library's Unicode version.

Tiktoken's [upstream collaborator explicitly confirms that its MIT license covers externally hosted encoding files](https://github.com/openai/tiktoken/issues/92#issuecomment-1497875652). A local copy of that clarification accompanies the retained MIT license for truncated encoding data in test fixtures. The historical GPT-2 modified MIT notice is retained too. Complete model weights and pretrained vocabularies are not included in these package releases.

The review inspected port source, manifests, oracle generators and generated fixtures; checked the exact source distributions named by the oracles where available; and compared port source against 80 relevant upstream implementation files. [source-audit.json](source-audit.json) records those references and fingerprints. Exact fragments and numerical coefficients exist, including einops fragments and SciPy numerical material. Their identified licenses allow copying and adaptation when their conditions are retained. Translations are treated conservatively as adaptations, without relying on them escaping upstream copyright.

The exact historical commit used to write every port was not recorded. Reference versions are oracle comparison versions, not claims of derivation from particular commits. This is a bounded technical audit, not proof of independent authorship of every line or a legal opinion. No identified upstream license prohibits the reuse reviewed here; no upstream ownership was reassigned to Nimblesite.

## Distribution and continuing checks

The publisher includes root `LICENSE`, `COPYING`, `NOTICE`, `AUTHORS` and `THIRD_PARTY_NOTICES` files, including `.txt`/`.md` forms, and the complete `LICENSES/` tree without asset declarations. They participate in `SourceDigest`, survive bundle/fetch/install, and tampering invalidates the bundle. Consumers distributing compiled programs must supply the applicable documents too; compilation does not embed them automatically.

[license-audit.json](license-audit.json) pins retained text SHA-256 hashes and upstream sources. Git blob hashes identify source documents; `source_selection` identifies complete license sections extracted from larger documents. Archive hashes identify downloaded releases. Identical standard license text within a package is stored once and referenced by every applicable component.

Run `node scripts/verify-package-licenses.mjs` from the repository root. It checks every package is inventoried, the Nimblesite MIT grant, upstream texts, SPDX expressions and modification notices. It also runs under `make lint` and `make ci`. Update the inventory and notices when incorporating upstream material; changing a hash alone does not establish permission.

Regression tests are in [publish_tests.osp](../registry/cli/tests/src/publish_tests.osp) and [fetch_tests.osp](../registry/cli/tests/src/fetch_tests.osp): `target/release/osprey test registry/cli/tests --filter licenses` must pass. They check legal-file retention, the published digest and rejection of altered attribution.
