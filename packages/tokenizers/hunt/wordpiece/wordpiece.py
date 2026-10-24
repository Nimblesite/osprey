# osprey: flavor=python
"""Hunt driver: WordPiece encode/decode round trip on one symbolic text line
(invalid UTF-8 included). The models' integer arithmetic is discharged by a
local value-mode `Arith` handler, so KLEE explores the greedy tokenizer, the
normalizer, the pre-tokenizer and the decoder instead of stopping at the
control-effect machinery (which the hunt mock does not model)."""
from testing import Check
from tokenizers.Models import build_wordpiece
from tokenizers.Tokenizer import wordpiece_decode_pipeline, wordpiece_encode

WP_VOCAB = ["[PAD]", "[UNK]", "[CLS]", "[SEP]", "[MASK]", "hello", "world", "##s", "##!", "!", "a", "b", "##b", "foo", "bar", "baz", "hi", "##llo"]



def run():
    wp = build_wordpiece(WP_VOCAB, "[UNK]", "##", 100)
    text = input()
    enc = wordpiece_encode(wp, text, 2, 3)
    dec = wordpiece_decode_pipeline(wp, enc.ids)
    return print(len(enc.ids) + len(enc.tokens) + len(dec))


def main():
    return Check.lenient(run)
