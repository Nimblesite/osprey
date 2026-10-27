# osprey: flavor=python
"""Hunt driver: Unigram (Viterbi + byte fallback) encode/decode round trip on
one symbolic text line (invalid UTF-8 included). The full `<0xNN>` byte
vocabulary exercises the byte-fallback path for every possible byte; a small
word vocabulary keeps the Viterbi DP's path count bounded."""
from testing import Check
from tokenizers.Models import build_unigram
from tokenizers.Tokenizer import unigram_decode_pipeline, unigram_encode_pipeline

UNI_WORDS = ["<unk>", "hello", "world", "a", "b", "c", "!", "foo", "bar"]
HEX = "0123456789ABCDEF"


def byte_text(b):
    """The `<0xNN>` spelling a byte-fallback piece uses."""
    return "<0x" + unwrap_or(substring(HEX, b // 16, b // 16 + 1), "0") + unwrap_or(substring(HEX, b % 16, b % 16 + 1), "0") + ">"


def uni_vocab():
    return listConcat(UNI_WORDS, toList(map(range(256), byte_text)))


def uni_scores():
    return listConcat(toList(map(range(len(UNI_WORDS)), lambda _: -1.0)), toList(map(range(256), lambda _: -10.0)))



def run():
    uni = build_unigram(uni_vocab(), uni_scores(), 0, True)
    text = input()
    enc = unigram_encode_pipeline(uni, text)
    dec = unigram_decode_pipeline(uni, enc.ids, True)
    return print(len(enc.ids) + len(dec))


def main():
    return Check.lenient(run)
