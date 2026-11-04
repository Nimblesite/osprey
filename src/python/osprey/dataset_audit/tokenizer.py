"""Offline Hugging Face token counts, with the actual chat template when needed."""

import importlib.metadata
from pathlib import Path

from .records import file_digest

TOKENIZER_FILES = {"tokenizer.json", "tokenizer_config.json", "special_tokens_map.json", "added_tokens.json",
                   "chat_template.jinja", "vocab.json", "vocab.txt", "merges.txt", "config.json"}


def load(directory):
    if directory is None:
        return None
    try:
        from transformers import AutoTokenizer
    except ImportError as error:
        raise ValueError("Install osprey-interop[dataset-audit] to use a local Hugging Face tokenizer") from error
    return AutoTokenizer.from_pretrained(str(directory), local_files_only=True, trust_remote_code=False)


def count(tokenizer, value, separator):
    if tokenizer is None:
        return None
    if value.messages is not None:
        if not tokenizer.chat_template:
            raise ValueError("The local tokenizer needs a chat template for chat-format data")
        return len(tokenizer.apply_chat_template(value.messages, tokenize=True, add_generation_prompt=False, return_dict=False))
    prompt = "\n\n".join(text for _, text in value.turns)
    return len(tokenizer.encode(prompt + separator + value.target, add_special_tokens=True, truncation=False))


def metadata(directory):
    if directory is None:
        return None
    load(directory)
    files = {path.name: file_digest(path) for path in sorted(Path(directory).iterdir())
             if path.is_file() and path.name in TOKENIZER_FILES}
    if not files:
        raise ValueError("The tokenizer directory has no supported tokenizer files")
    return {"directory": str(directory), "files": files, "transformers": importlib.metadata.version("transformers"),
            "chat": "model_chat_template", "pairs": "prompt + configured separator + completion"}
