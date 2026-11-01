# osprey: flavor=python
"""Greedy text generation with the trained GPT-2 model, in the Python flavor.

`make -C apps gpt2 FLAVOR=python PROMPT="..."` downloads the published weights into
examples/gpt2/weights and runs this project. Three lines of stdin: the prompt,
the number of tokens to generate (default 20), and the interpretation to run
the model under: `gpu` for the GPU surface (its products on the device
`OSPREY_GPU` selects) or `lists` for plain host lists. The model lives in
../model.py and performs the `Tensor` effect; this file is the sampler, and
`main` is the one place that chooses a handler. The prompt takes one full
forward pass; every later token takes one forward_step over the blocks' cached
qkv rows, so a token costs one position rather than the whole sequence.
"""
from gpt2 import Backends, Model, Tokenizer
from gpt2.Backends import Tensor
from gpt2.Model import at, max_f, WIDTH, VOCAB_SIZE, LAYER_COUNT, END_OF_TEXT

DEFAULT_PROMPT = "The capital of France is"
DEFAULT_TOKENS = 20


def next_token(w, last):
    """The most likely token after the last block's output row `last` [1, width]:
    the tied output projection of its normalised state, then the first index
    holding its peak; END_OF_TEXT when none does."""
    logits = Model.product(Model.layer_norm(last, 1, w.lnf_g, w.lnf_b), w.wte_t, 1, WIDTH, VOCAB_SIZE)
    found = perform(Tensor.index_of(logits, perform(Tensor.reduce(logits, at(logits, 0), max_f))))
    return found if found < VOCAB_SIZE else END_OF_TEXT


def last_row(x, t):
    """Row t - 1 of x [t, width]."""
    return perform(Tensor.build(WIDTH, lambda c: at(x, (t - 1) * WIDTH + c)))


def generate(w, ids, caches, pending, remaining):
    """`remaining` more tokens after ids, whose last token `pending` the cached
    blocks have not seen yet."""
    if remaining == 0:
        return ids
    else:
        step = Model.forward_step(w, pending, len(ids), caches)
        token = next_token(w, step.x)
        return ids if token == END_OF_TEXT else generate(w, listAppend(ids, token), step.caches, token, remaining - 1)


def continuation(w, ids, trace, t, count):
    """The prompt's continuation: the full pass's last row predicts the first token,
    and the pass's qkv rows seed the caches for every later one."""
    first = next_token(w, last_row(trace.x, t))
    return ids if first == END_OF_TEXT else generate(w, listAppend(ids, first), Model.qkv_caches(trace), first, count - 1)


def run(prompt, count):
    """Load, tokenize, generate and print, under whichever interpretation the caller
    installed."""
    bpe = Tokenizer.load_bpe(Model.WEIGHTS_DIR + "/merges.txt")
    w = Model.load_weights()
    ids = Tokenizer.encode(bpe, prompt)
    t = len(ids)
    print(f"gpt2 (124M) on {Model.device()} — {len(bpe.merges)} merges, {Model.size(w.wte) // WIDTH} vocabulary rows, {LAYER_COUNT} layers\nprompt: {prompt}\ntokens:{Tokenizer.ids_text(ids)}")
    out = ids if count < 1 else continuation(w, ids, Model.forward(w, ids, t), t, count)
    print(f"generated {len(out) - len(ids)} tokens\n{Tokenizer.decode(bpe, out)}")


def main():
    prompt_line = input()
    prompt = DEFAULT_PROMPT if prompt_line == "" else prompt_line
    count = unwrap_or(parseInt(input()), DEFAULT_TOKENS)
    backend = input()
    match Backends.strictly(lambda: Backends.under(backend, lambda: run(prompt, count))):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"generation stopped: {message}")
