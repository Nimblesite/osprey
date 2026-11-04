# osprey: flavor=python
"""Hunt driver: byte-pair encode/decode round trip on one symbolic input line.

The vocabulary is fixed and concrete (trained on a tiny corpus), so every
branch KLEE explores is driven by the symbolic input bytes: the pre-tokenizer
split, the merge loop, and the decoder's UTF-8 error paths. A lenient Arith
handler answers the integer divisions of UTF-8 encoding with values, the way
[PORT-HUNT] requires."""
from testing import Check
from tiktoken import Bpe, Core, Educational, Pattern


def vocab():
    """A small concrete vocabulary trained on a fixed corpus."""
    match Educational.bpe_train("hello world hello there", 262, Pattern.Gpt2()):
        case Success(value=value):
            return value
        case Error(message=message):
            return Bpe.base_ranks()


def run():
    enc = Core.build("hunt", vocab(), {}, Pattern.Gpt2())
    return round_trip(enc, input())


def round_trip(enc, text):
    """Encode text, decode the ids, and print both lengths."""
    match Core.encode_ordinary(enc, text):
        case Success(value=value):
            return decode_of(enc, value)
        case Error(message=message):
            return print(message)


def decode_of(enc, ids):
    """Decode token ids, printing the round-trip length; Error on failure."""
    match Core.decode(enc, ids):
        case Success(value=value):
            return print(len(ids) + len(value))
        case Error(message=message):
            return print(message)


def main():
    return Check.lenient(run)
