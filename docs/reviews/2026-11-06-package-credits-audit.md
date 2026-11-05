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

- The 25 October token-similarity comparison was not re-run for code added since. Only the self-described ports and the files found above were checked by hand.
- `osprey main.py --check` in `transformers/tests` reports `unknown symbol path Qwen3_5Device::strictly` both at HEAD and with these changes. It is unrelated to this audit and not investigated.
