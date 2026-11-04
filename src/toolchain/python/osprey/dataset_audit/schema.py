"""Text SFT schemas, canonical prompt identity and a distinct completion identity."""

from dataclasses import dataclass
import hashlib
import json
import unicodedata

ROLES = {"system", "developer", "user", "assistant", "tool"}


@dataclass(frozen=True)
class Document:
    kind: str
    turns: tuple[tuple[str, str], ...]
    target: str
    messages: list | None

    @property
    def chars(self):
        return sum(len(text) for _, text in self.turns) + len(self.target)


def document(record):
    if not isinstance(record, dict):
        raise ValueError("not_an_object")
    matches = [("messages" in record, _chat), ("prompt" in record and "completion" in record, _pair),
               ("instruction" in record and "output" in record, _instruction),
               ("instruction" in record and "response" in record, _dolly)]
    selected = [reader for present, reader in matches if present]
    if len(selected) != 1:
        raise ValueError("unknown_or_ambiguous_schema")
    result = selected[0](record)
    if not any(role == "user" and text.strip() for role, text in result.turns):
        raise ValueError("empty_prompt")
    return result


def _text(record, name, default=None):
    value = record.get(name, default)
    if not isinstance(value, str):
        raise ValueError("non_text_field")
    return value


def _pair(record):
    return Document("prompt_completion", (("user", _text(record, "prompt")),), _text(record, "completion"), None)


def _instruction(record):
    prompt = "\n\n".join(text for text in [_text(record, "instruction"), _text(record, "input", "")] if text)
    return Document("instruction", (("user", prompt),), _text(record, "output"), None)


def _dolly(record):
    prompt = "\n\n".join(text for text in [_text(record, "instruction"), _text(record, "context", "")] if text)
    return Document("dolly", (("user", prompt),), _text(record, "response"), None)


def _chat(record):
    messages = record["messages"]
    if not isinstance(messages, list) or len(messages) < 2:
        raise ValueError("invalid_messages")
    turns = tuple(_turn(message) for message in messages)
    if turns[-1][0] != "assistant":
        raise ValueError("missing_assistant_completion")
    return Document("chat", turns[:-1], turns[-1][1], messages)


def _turn(message):
    if not isinstance(message, dict) or message.get("role") not in ROLES:
        raise ValueError("invalid_message_role")
    return message["role"], _text(message, "content")


def normalize(text, mode):
    return " ".join(unicodedata.normalize("NFKC", text).split()) if mode == "whitespace" else text


def identities(value, mode):
    prompt = json.dumps([(role, normalize(text, mode)) for role, text in value.turns], ensure_ascii=False, separators=(",", ":"))
    target = normalize(value.target, mode)
    return prompt, digest(prompt), digest(target)


def digest(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()
