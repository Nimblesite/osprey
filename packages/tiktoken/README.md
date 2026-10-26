# @osprey/tiktoken

A byte-level BPE tokenizer port of [tiktoken](https://github.com/openai/tiktoken) 0.14.0 to Osprey's Python flavor ([spec 0043](../../docs/specs/ecosystem/0043-PythonFlavorPorts.md)). It generalises `examples/gpt2/python/tokenizer.py` into tiktoken's `Encoding` API, with the same pre-tokenizer patterns, special-token policy and `_educational` training loop, all hand-written over code points (no regex engine, no Python interop).

The vocabulary is a string-keyed map: token bytes become one canonical key by shifting each byte to the code point `b + 1` (code point 0 cannot live in an Osprey string), so concatenating keys is the key of the concatenated bytes — which makes a merge a string concatenation. The suite's expected values are computed by the real tiktoken (`scripts/oracles/tiktoken.py`, tiktoken 0.14.0, `regex` 2026.9.29, Unicode 15.0.0).

## API

| Function | Original | Notes |
| --- | --- | --- |
| `Core.Encoding` (record) | `tiktoken.Encoding` | `name`, `ranks` (canonical keys), `special_tokens`, `pattern`, `max_token_value`, `n_vocab`, `entries` |
| `Core.build(name, ranks, specials, pattern)` | `tiktoken.Encoding(...)` | Derives `max_token_value`, `n_vocab` and the rank-sorted entries |
| `Core.encode_ordinary(enc, text)` | `Encoding.encode_ordinary` | Returns `Result[list[int], str]` (the original returns a plain list) |
| `Core.encode(enc, text, allowed)` | `Encoding.encode` | A disallowed special token is an `Error`, not `ValueError` |
| `Core.encode_single_token(enc, text)` | `Encoding.encode_single_token` | `Error` instead of `KeyError` |
| `Core.decode(enc, tokens)` | `Encoding.decode` | `errors="replace"`; `Result[str, str]` instead of raising |
| `Core.decode_errors(enc, tokens, errors)` | `Encoding.decode(errors=...)` | `replace`, `ignore`, `strict` |
| `Core.decode_bytes(enc, tokens)` | `Encoding.decode_bytes` | `Result[list[int], str]` |
| `Core.decode_single_token_bytes(enc, token)` | `Encoding.decode_single_token_bytes` | `Result[list[int], str]` |
| `Core.decode_tokens_bytes(enc, tokens)` | `Encoding.decode_tokens_bytes` | `Result[list[list[int]], str]` |
| `Core.token_byte_values(enc)` | `Encoding.token_byte_values` | Lexicographic byte order, specials excluded |
| `Core.is_special_token(enc, token)` | `Encoding.is_special_token` | |
| `Core.special_tokens_set(enc)` | `Encoding.special_tokens_set` | A sorted list (the original's is a set) |
| `Educational.simple(pattern, ranks)` | `SimpleBytePairEncoding(...)` | |
| `Educational.bpe_encode(ranks, piece)` | `tiktoken._educational.bpe_encode` | `Result[list[int], str]` |
| `Educational.bpe_train(data, vocab, pattern)` | `tiktoken._educational.bpe_train` | Exact merge order, ties broken like `Counter` + `max` |
| `Educational.train(...)` | `SimpleBytePairEncoding.train` | |
| `Pattern.Gpt2()` / `Pattern.Cl100kBase()` | the GPT-2 and cl100k `pat_str` | Hand-written scanners |
| `Pattern.split(pattern, text)` | `regex.findall(pat_str, text)` | Pieces as UTF-8 byte lists |

The `Unicode` and `Bpe` modules are internal helpers (code point classification, UTF-8 encode/decode, the canonical key codec, the merge core and the special-token scan); `Tables` holds the `\p{L}`/`\p{N}`/`\s` range tables the pre-tokenizers need.

## Divergences

- **`Result` instead of exceptions.** tiktoken raises `ValueError` (disallowed special), `KeyError` (unknown token) and `UnicodeDecodeError` (`strict`); the port returns `Result[T, str]`. `encode_ordinary` returns `Result[list[int], str]` where the original returns `list[int]` — the broken-vocabulary case that panics in the Rust core is an `Error` here.
- **NUL.** U+0000 cannot appear in an Osprey string (NUL-terminated), so decoding token byte 0 returns `Error` where tiktoken returns `"\x00"`.
- **Unicode class tables cover a fixed block set.** Code points outside the blocks in `Tables` classify as punctuation; the corpus and battery stay inside, and the suite pins the divergence for a few outsiders. Regenerate with `scripts/oracles/tiktoken.py`.
- **No regex engine.** `\p{L}`, `\p{N}`, `\s` and the two patterns are hand-written scanners over code points, pinned against the real `regex` module's `findall` on a multilingual battery.
- **cl100k contraction case-folding** implements simple case folding for ASCII and U+017F (long s); the suite pins it against the real `(?i:...)`.
- **`special_tokens_set` returns a sorted list**, not a `set`.

## Bug hunts

`hunt/encode` is an `osprey hunt` driver ([PORT-HUNT]): it trains a small concrete vocabulary and round-trips encode/decode on one symbolic stdin line. `osprey hunt packages/tiktoken/hunt/encode --max-time 120` explored 2052 paths (16 complete) with no findings; the 2036 incomplete paths were cut by the time budget or a query timeout, not bugs. The driver discharges `Arith` with `Check.lenient` and installs no `@control` handler ([SYMEX-MOCK-UNSUPPORTED](../../docs/specs/tooling/SymbolicExecution.md)).
