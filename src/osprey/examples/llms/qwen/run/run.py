# osprey: flavor=python
"""Qwen3.5 on real GGUF weights through the package model: the greedy continuation of a prompt.

Three lines on stdin: the GGUF path, the prompt, and how many tokens to add. The
tokenizer is built from the file's own `tokenizer.ggml.*` tables, the model is
`Qwen3_5Gguf.load_path`, the prompt goes in as one prefill (`prefill N -> next`) and
every generated token prints as `position token -> next`, so a run is its own receipt ([LLM-TOKENIZER], [LLM-WEIGHTS-MAPPED], [LLM-MODEL-ORACLE]).
A prompt line `ids: 1 2 3` feeds those token ids as they are, and a count line
`logits: 7 8 9` adds nothing: it prints the raw logits at those ids after the prompt as
`logits best B id=value ...`, the Osprey half of the llama.cpp comparison
(`eval/logits.py`, [LLM-MODEL-ORACLE]):

    printf '%s\\n%s\\n%s\\n' ~/.cache/osprey-llm/gguf/Qwen3.8-27B-UD-Q4_K_XL.gguf 'The capital of France is' 8 \\
      | OSPREY_GPU=metal osprey src/osprey/examples/llms/qwen/run --run --memory=arc
"""
from qwen import Qwen3_5Cache, Qwen3_5Device, Qwen3_5Gguf, Qwen3_5Model, Qwen3_5Vocab
from tokenizers import Tokenizer
from tokenizers.Tokenizer import qwen_decode


class Run:
    """The generation so far: the cache, the last token chosen, every token added, or the step that failed."""
    cache: Qwen3_5Cache.Cache
    best: int
    added: list[int]
    failed: str


def step(model, state, pos):
    """One generated token through the model after the last best."""
    if state.failed != "":
        return state
    else:
        return stepped(model, state, state.best, pos)


def stepped(model, state, token, pos):
    match Qwen3_5Model.decode(model, state.cache, token):
        case Success(value=next):
            print(f"{pos} {token} -> {next.best}")
            return Run(cache=next.walk.cache, best=next.best, added=listAppend(state.added, next.best), failed="")
        case Error(message=message):
            return Run(cache=state.cache, best=state.best, added=state.added, failed=f"step {pos}: {message}")


def generated(model, prompt, count):
    """`count` greedy tokens after `prompt`, the prompt taken in by one prefill
    ([LLM-PREFILL]), or the step that failed."""
    match resultAndThen(Qwen3_5Cache.empty(model.config, model.shapes), lambda cache: Qwen3_5Model.prefill(model, cache, prompt)):
        case Error(message=message):
            return Error(message=message)
        case Success(value=next):
            print(f"prefill {len(prompt)} -> {next.best}")
            start = Run(cache=next.walk.cache, best=next.best, added=[next.best], failed="")
            return finished(fold(range(len(prompt), len(prompt) + count - 1), start, lambda state, pos: step(model, state, pos)))


def probed(model, prompt, ids):
    """The prompt's next-token argmax and its raw logits at `ids`, after one prefill."""
    match resultAndThen(Qwen3_5Cache.empty(model.config, model.shapes), lambda cache: Qwen3_5Model.prefill(model, cache, prompt)):
        case Error(message=message):
            return Error(message=message)
        case Success(value=next):
            print(f"logits best {next.best} {join(mapList(ids, lambda id: f'{id}={gpuGet(next.logits, id)}'), ' ')}")
            return Success(value=[next.best])


def finished(run):
    return Success(value=run.added) if run.failed == "" else Error(message=run.failed)


def spelled(ids):
    return join(mapList(ids, toString), " ")


def numbers(text):
    """The integers of a space-separated line, or the first word that is not one."""
    found = words(text)
    return fold(range(0, len(found)), Success(value=[]), lambda acc, i: resultAndThen(acc, lambda got: resultAndThen(nth_count(found, i), lambda n: Success(value=listAppend(got, n)))))


def nth_count(found, i):
    match listGet(found, i):
        case Success(value=word):
            return count_of(word)
        case Error(message=message):
            return Error(message=f"ids: {toString(message)}")


def count_of(text):
    match parseInt(text):
        case Success(value=n):
            return Success(value=n)
        case Error(message=message):
            return Error(message=f"token count: {toString(message)}")


class Job:
    """What stdin asked for, once the file is open and the prompt tokenized."""
    source: Qwen3_5Gguf.Source
    tok: Tokenizer.QwenTokenizer
    prompt: list[int]
    count: int
    probe: list[int]


def prompt_of(tok, text):
    """`ids: …` as given, any other line through the file's tokenizer."""
    return numbers(drop(text, 4)) if startsWith(text, "ids:") else Qwen3_5Vocab.ids_of(tok, text)


class Wanted:
    """How many tokens to add, or the ids whose logits to print instead."""
    count: int
    probe: list[int]


def asked(count):
    """`logits: …` as the ids to probe with nothing added, else a token count and no probe."""
    return resultAndThen(numbers(drop(count, 7)), lambda ids: Success(value=Wanted(count=0, probe=ids))) if startsWith(count, "logits:") else resultAndThen(count_of(count), lambda n: Success(value=Wanted(count=n, probe=[])))


def job(path, text, count):
    return resultAndThen(Qwen3_5Gguf.open_file(path), lambda source: resultAndThen(Qwen3_5Vocab.tokenizer(source.file), lambda tok: resultAndThen(prompt_of(tok, text), lambda prompt: resultAndThen(asked(count), lambda wanted: Success(value=Job(source=source, tok=tok, prompt=prompt, count=wanted.count, probe=wanted.probe))))))


def answered(j):
    """Load, generate, decode: the text the model added."""
    print(f"prompt ids {spelled(j.prompt)}")
    return resultAndThen(Qwen3_5Gguf.load(j.source), lambda model: resultAndThen(loaded(model, j), lambda added: resultAndThen(qwen_decode(j.tok, added, False), lambda text: Success(value=f"added ids {spelled(added)}\ntext <<<{text}>>>"))))


def loaded(model, j):
    print("loaded")
    return probed(model, j.prompt, j.probe) if len(j.probe) > 0 else generated(model, j.prompt, j.count)


def run():
    with Qwen3_5Device.wrapping:
        return resultAndThen(job(input(), input(), input()), lambda j: answered(j))


def main():
    match run():
        case Success(value=text):
            print(text)
            return 0
        case Error(message=message):
            print(f"failed: {message}")
            return 1
