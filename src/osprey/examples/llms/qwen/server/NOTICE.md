# Qwen3.5 server: licensing and upstream acknowledgement

Nimblesite's own code here is provided under the MIT license, Copyright (c) 2026 NIMBLESITE PTY LTD, Australia. This does not claim ownership of any third-party material identified below; the cumulative expression for this directory is `MIT AND Apache-2.0`.

## Specifically retained third-party material

`tests/chat_template.jinja` is the unmodified `tokenizer.chat_template` metadata of [`unsloth/Qwen3.8-27B-GGUF`](https://huggingface.co/unsloth/Qwen3.8-27B-GGUF/tree/4ca720788d1e01f1bff70c033e0d0028fd02e502) at revision `4ca720788d1e01f1bff70c033e0d0028fd02e502` (file `Qwen3.8-27B-UD-Q4_K_XL.gguf`), Unsloth's build of the [Qwen3.8-27B](https://huggingface.co/Qwen/Qwen3.8-27B) chat template published by the Qwen team, Alibaba Cloud, under the Apache-2.0 license. See [`../model/LICENSES/Qwen3.8-27B-Apache-2.0.txt`](../model/LICENSES/Qwen3.8-27B-Apache-2.0.txt).

Its SHA-256 is `12827f24b742ea4e80cdc12dbcf9622227056b9f797252a3149263d4f9aaadce` (`shasum -a 256 tests/chat_template.jinja`), and `tests/model_reference.cpython` renders through it so the served prompt is checked against the template itself rather than against a string the server could have been handed.
