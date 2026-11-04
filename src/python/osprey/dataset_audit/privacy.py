"""Named heuristic findings; matched values never enter reports or callbacks."""

import re

PATTERNS = {
    "secret.aws_access_key": re.compile(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b"),
    "secret.github_token": re.compile(r"\bgh[pousr]_[A-Za-z0-9]{36,}\b"),
    "secret.api_token": re.compile(r"\bsk-[A-Za-z0-9_-]{20,}\b"),
    "secret.private_key": re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    "pii.email": re.compile(r"[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?\.[A-Za-z]{2,}"),
    "pii.international_phone": re.compile(r"(?<!\w)\+[1-9][0-9 ()-]{8,24}[0-9](?!\w)"),
}
SECRET_FIELDS = {"password", "api_key", "access_token", "client_secret", "private_key"}
REDACTED = {"<redacted>", "[redacted]", "<api_key>", "your_api_key"}


def findings(record):
    found = set()
    for key, text in strings(record):
        text.encode("utf-8")
        found.update(name for name, pattern in PATTERNS.items() if pattern.search(text))
        if key.lower() in SECRET_FIELDS and len(text) >= 8 and text.lower() not in REDACTED:
            found.add("secret.credential_field")
    return sorted(found)


def strings(record):
    pending = [("", record, 0)]
    while pending:
        key, value, depth = pending.pop()
        if depth > 64:
            raise ValueError("excessive_nesting")
        if isinstance(value, str):
            yield key, value
        elif isinstance(value, dict):
            pending.extend((name, child, depth + 1) for name, child in value.items())
        elif isinstance(value, list):
            pending.extend((key, child, depth + 1) for child in value)
