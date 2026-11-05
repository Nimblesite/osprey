# Package credits and license audit, 6 November 2026

Scope: every package under `src/osprey/packages/` (27 audited packages; `manager` is Osprey's own package manager and has no upstream). The check covered each README, `NOTICE.md`, `LICENSES/` and `license-audit.json` entry. Upstream licenses were read from each GitHub repository's license metadata on 6 November.

## Findings and fixes

| Package | Finding | Fix |
| --- | --- | --- |
| all ports | No README named its original repository's license or thanked its authors. Most did not link the original. | Each README has an "Origin, license and thanks" section: original project and link, its license, how the Python-flavor port relates to it, and thanks. Original Osprey packages say they are original. |
| chromecast | NOTICE said the reference script was "written for this project" and README never said where `cast.py` came from. | Traced in the session logs: Claude wrote `cast.py` for the maintainer on 31 October 2026, from its own protocol knowledge, with no third-party code installed, fetched or copied. NOTICE and README now say so, name the Google trademarks and credit public protocol work (pychromecast, catt) without claiming derivation. |
| transformers | The Qwen3.5 model modules (added 5 November, after the 25 October audit) are translations of Apache-2.0 `modeling_qwen3_5.py`, but NOTICE said no shared implementation existed and no Apache terms were attached to them. | Treated as derived: Apache-2.0 text in `LICENSES/transformers-Apache-2.0.txt`, upstream copyright and a "changed for Osprey" notice at the top of each of the nine files (Apache-2.0 §4(b), §4(c)), an audit upstream entry enforced by `verify-package-licenses.mjs`, and NOTICE naming the files and the changes. vLLM's tool parser is credited as a behavior reference. |
| tokenizers | `src/NfcData.py` and `src/Category.py` hold Unicode data recovered from tokenizers 0.23.3 and point at NOTICE, but NOTICE covered only `src/Unicode.py`. | Unicode License v3 retained and listed for both files; expression `MIT AND Unicode-DFS-2016 AND Unicode-3.0`. Their Unicode data version is not recorded. |
| fastapi | `src/applications.py` adapts Starlette's BSD-3-Clause CORS middleware, and its notice is retained, but the expression was `MIT`. | Expression `MIT AND BSD-3-Clause` in the manifest, audit and NOTICE. |

## How READMEs describe the ports

Osprey's Python flavor reads like Python, so ports keep upstream names and call shapes and are tested against the upstream package under CPython. The READMEs say this makes a port closer to the original than usual, but call it a reimplementation only where the audit found no shared implementation. Where code was translated (the Qwen3.5 modules) or adapted (FastAPI, Starlette CORS, ggml tables, SciPy kernels, micrograd in its oracle), the README says so and the upstream terms apply to that material. The MIT grant is stated to cover Osprey's own code only and never to relicense upstream code. Naming a project is credit, not a claim of endorsement.

## Not done

- The 25 October token-similarity comparison was re-run on 6 November; see below.
- `osprey main.py --check` in `transformers/tests` reports `unknown symbol path Qwen3_5Device::strictly` both at HEAD and with these changes. It is unrelated to this audit and not investigated.

## Second check: MIT on ports of BSD-3 and Apache-2.0 projects

The concern was whether a package may carry MIT when its upstream is BSD-3-Clause (NumPy, scikit-learn, PyTorch, FlashAttention, SciPy, Starlette) or Apache-2.0 (rouge-score, sacreBLEU, safetensors, Tokenizers, Transformers).

That is allowed on two conditions. First, MIT may only cover code that NIMBLESITE wrote. Second, any code actually derived from the upstream keeps that upstream's copyright notice, conditions and disclaimer, and the package's license expression includes that upstream's license. It would not be allowed to strip a BSD or Apache notice from derived code and relabel it MIT-only, or to use an upstream's name to suggest it endorses the port (BSD-3-Clause condition 3).

The packages that contain derived material already work this way:

| Package | License expression |
| --- | --- |
| fastapi | `MIT AND BSD-3-Clause` |
| scipy | `MIT AND BSD-3-Clause AND BSL-1.0` |
| transformers | `MIT AND Apache-2.0` |
| tokenizers and tiktoken | MIT plus Unicode terms |
| gguf | carries the ggml tables under llama.cpp MIT |
| jinja2 | carries the Apache-2.0 Qwen template |

The MIT-only packages are MIT-only because they are reimplementations. To test that claim, the 25 October similarity comparison was re-run today on the current tree. The upstreams compared were the installed or downloaded releases of NumPy, SciPy, scikit-learn, PyTorch, safetensors, Tokenizers, Transformers, Jinja2, rouge-score 0.1.2, sacreBLEU 2.6.0, einops 0.8.2, faiss-cpu 1.15.1, pydantic-ai-slim 2.66.0, FastAPI 0.162.0, Starlette 1.10.0 and gguf 0.19.0. The comparison checked shared non-trivial lines of 45 characters or more, and shared runs of five consecutive lines.

The result:

- No package shares a five-line run with its upstream.
- Two single lines matched: a list of NumPy ufunc names, and one `if` condition in einops. Neither is protectable expression.
- FlashAttention's upstream is CUDA/C++ and was not compared line by line. Its port is written from the algorithm.

The `verify-package-licenses` gate passes for all 27 packages.

Residual risk:

- Python flavor keeps upstream API names and module layout. In the US that question was resolved for Google on fair use (Google v. Oracle, 2021), not on whether APIs are copyrightable. Australian law has no equivalent ruling.
- Package names such as `numpy` and `torch` raise a trademark question, not a copyright one. The READMEs state that no endorsement is implied.
- This is an engineering audit, not legal advice.
