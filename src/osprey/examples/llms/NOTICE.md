# LLM examples: third-party material

The Osprey code here is under the repository's [MIT license](../../../../LICENSE). These parts come from other authors, under their own terms.

| Material | Files | Upstream | Terms |
| --- | --- | --- | --- |
| Byte-level BPE tokenizer: the byte alphabet, split pattern and merge loop, adapted | `gpt2/reference.py`, `gpt2/python/tokenizer.py`, `gpt2/model/tokenizer.osp` | [`src/encoder.py`](https://github.com/openai/gpt-2/blob/master/src/encoder.py) in OpenAI's GPT-2, Software Copyright (c) 2019 OpenAI | [Modified MIT](LICENSES/gpt-2-Modified-MIT.txt) |
| Q4_K block layout and 6-bit scale and minimum unpacking, adapted | `qwen/speed/` | `get_scale_min_k4` and `block_q4_K` in [llama.cpp](https://github.com/ggml-org/llama.cpp), Copyright (c) 2023-2026 The ggml authors | [MIT](LICENSES/llama.cpp-MIT.txt) |
| Detokenization replacement table, adapted, in the `wordpiece-decode` task | `qwen/eval/coding_tasks.json` | `clean_up_tokenization` from Google's [BERT](https://github.com/google-research/bert), as carried by Hugging Face Transformers | [Apache-2.0](LICENSES/bert-Apache-2.0.txt) |

Model weights are not in this repository. Downloads carry the terms their publishers set.
