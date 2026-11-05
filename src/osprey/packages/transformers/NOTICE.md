# @osprey/transformers: licensing and upstream acknowledgement

The Osprey implementation is provided under the normal [MIT license](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia, retaining the earlier Christian Findlay notice. This does not claim ownership of any third-party material identified below.

## Upstream API and behavior references

These repositories and releases are acknowledged as API, behavior and test-oracle references. Their licenses are not assigned to this implementation merely because the API is reimplemented. The port does not bundle or link the upstream Python package.

- [transformers](https://github.com/huggingface/transformers): 5.20.2.

## Specifically retained third-party material

`oracle/qwen3_5_chat_template.jinja` is the unmodified chat template published with [Qwen3.8-27B](https://huggingface.co/Qwen/Qwen3.8-27B/tree/1d4bf0f2ff6012fd82039f2fa52739d0dd7c60c0) by the Qwen team, Alibaba Cloud, under the Apache-2.0 license. See [`LICENSES/Qwen3.8-27B-Apache-2.0.txt`](LICENSES/Qwen3.8-27B-Apache-2.0.txt). The cumulative expression is `MIT AND Apache-2.0`.

The Qwen3.5 text model in `src/qwen3_5_config.py`, `src/qwen3_5_norm.py`, `src/qwen3_5_rope.py`, `src/qwen3_5_mlp.py`, `src/qwen3_5_attention.py`, `src/qwen3_5_deltanet.py`, `src/qwen3_5_cache.py`, `src/qwen3_5_model.py` and `src/qwen3_5_hf.py` is translated from `src/transformers/models/qwen3_5/modeling_qwen3_5.py` in [Transformers](https://github.com/huggingface/transformers) 5.20.2, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, under the Apache License 2.0 in [`LICENSES/transformers-Apache-2.0.txt`](LICENSES/transformers-Apache-2.0.txt). Those files follow the upstream modules' structure, names and arithmetic, so they are treated as derived from that source, not as an independent implementation. They were changed: rewritten from PyTorch into Osprey Python-flavor GPU code, restricted to one-token decoding, with the cache made an explicit value and GGUF weight layouts added. Each carries a header saying so. The Apache-2.0 terms apply to those files; the MIT grant covers Nimblesite's own contributions and does not relicense the upstream code.

[vLLM](https://github.com/vllm-project/vllm)'s `Qwen3CoderToolParser` (Apache-2.0) was the reference for the tool-call framing rules in `src/qwen3_5_tool_parser.py`; no vLLM code is included.

## License scope

No additional upstream software license is imposed based solely on the API name, mathematical behavior, output of an oracle, short compatibility diagnostics or the label “port”. Apart from the files named above, no substantive shared implementation block was identified in the recorded comparison. A matching API does not establish either copying or independent authorship; review any future incorporation of upstream source or data separately.

## Distribution

The publisher carries `LICENSE`, this acknowledgement and any required `LICENSES/` files in the digest-pinned release. Source or compiled redistributions containing the specifically retained material must carry its applicable notices. Upstream project names provide acknowledgement and imply no endorsement.
