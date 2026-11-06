# Qwen3.5 model: licensing and upstream acknowledgement

Nimblesite's own code here is provided under the MIT license, Copyright (c) 2026 NIMBLESITE PTY LTD, Australia. This does not claim ownership of any third-party material identified below; the cumulative expression for this directory is `MIT AND Apache-2.0`.

## Specifically retained third-party material

`oracle/chat_template.jinja` is the unmodified chat template published with [Qwen3.8-27B](https://huggingface.co/Qwen/Qwen3.8-27B/tree/1d4bf0f2ff6012fd82039f2fa52739d0dd7c60c0) by the Qwen team, Alibaba Cloud, under the Apache-2.0 license. See [`LICENSES/Qwen3.8-27B-Apache-2.0.txt`](LICENSES/Qwen3.8-27B-Apache-2.0.txt).

The Qwen3.5 text model in `src/config.py`, `src/norm.py`, `src/rope.py`, `src/mlp.py`, `src/attention.py`, `src/deltanet.py`, `src/cache.py`, `src/model.py` and `src/hf.py` is translated from `src/transformers/models/qwen3_5/modeling_qwen3_5.py` in [Transformers](https://github.com/huggingface/transformers) 5.20.2, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, under the Apache License 2.0 in [`LICENSES/transformers-Apache-2.0.txt`](LICENSES/transformers-Apache-2.0.txt). Those files follow the upstream modules' structure, names and arithmetic, so they are treated as derived from that source, not as an independent implementation. They were changed: rewritten from PyTorch into Osprey Python-flavor GPU code, restricted to one-token decoding, with the cache made an explicit value and GGUF weight layouts added. Each carries a header saying so. The Apache-2.0 terms apply to those files; the MIT grant covers Nimblesite's own contributions and does not relicense the upstream code.

[vLLM](https://github.com/vllm-project/vllm)'s `Qwen3CoderToolParser` (Apache-2.0) was the reference for the tool-call framing rules in `src/tool_parser.py`; no vLLM code is included.

## Distribution

Source or compiled redistributions containing the material above must carry its applicable notices and the files in [`LICENSES/`](LICENSES/). Upstream project names provide acknowledgement and imply no endorsement.
