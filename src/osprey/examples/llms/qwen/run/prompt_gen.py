"""Token ids for a prompt, and text for ids, from the GGUF's own vocabulary and merges, until
@osprey/tokenizers reads them itself. `prompt_gen.py <gguf> encode "text"` or `decode 1,2,3`."""
import sys

import gguf
from tokenizers import Tokenizer, decoders, models, pre_tokenizers

reader = gguf.GGUFReader(sys.argv[1])
fields = reader.fields
tokens = [str(bytes(fields["tokenizer.ggml.tokens"].parts[i]), "utf-8") for i in fields["tokenizer.ggml.tokens"].data]
merges = [tuple(str(bytes(fields["tokenizer.ggml.merges"].parts[i]), "utf-8").split(" ", 1)) for i in fields["tokenizer.ggml.merges"].data]
tok = Tokenizer(models.BPE(vocab={t: i for i, t in enumerate(tokens)}, merges=merges))
tok.pre_tokenizer = pre_tokenizers.ByteLevel(add_prefix_space=False, use_regex=True)
tok.decoder = decoders.ByteLevel()
if sys.argv[2] == "encode":
    print(tok.encode(sys.argv[3]).ids)
else:
    print(tok.decode([int(n) for n in sys.argv[3].split(",")], skip_special_tokens=False))
