"""Validated, reproducible settings; source files are always read-only."""

from dataclasses import dataclass, replace
import math
from pathlib import Path


@dataclass(frozen=True)
class Config:
    train: tuple[Path, ...]
    output: Path
    evaluation: tuple[Path, ...] = ()
    compiler: str | None = None
    batch_size: int = 2000
    worker_timeout: int = 300
    max_line_bytes: int = 1024 * 1024
    max_chars: int = 32000
    min_completion_chars: int = 1
    tokenizer_dir: Path | None = None
    max_tokens: int | None = None
    separator: str = "\n\n"
    near_threshold: float | None = None
    normalization: str = "whitespace"
    allow_personal_data: bool = False
    keep_database: bool = False


def validate(config):
    _validate_limits(config)
    train = tuple(Path(path).resolve(strict=True) for path in config.train)
    evaluation = tuple(Path(path).resolve(strict=True) for path in config.evaluation)
    paths = train + evaluation
    if not train or any(not path.is_file() for path in paths):
        raise ValueError("Provide at least one training JSONL file; inputs must be regular files")
    identities = [(path.stat().st_dev, path.stat().st_ino) for path in paths]
    if len(set(identities)) != len(paths):
        raise ValueError("Input files must be unique and cannot belong to both splits")
    tokenizer = Path(config.tokenizer_dir).resolve(strict=True) if config.tokenizer_dir else None
    if tokenizer and not tokenizer.is_dir():
        raise ValueError("The tokenizer must be a local directory")
    return replace(config, train=train, evaluation=evaluation, output=Path(config.output).resolve(), tokenizer_dir=tokenizer)


def _validate_limits(config):
    limits = [("batch_size", 1, 10000), ("worker_timeout", 1, 86400), ("max_line_bytes", 1, 128 * 1024 * 1024),
              ("max_chars", 0, 2**63 - 1), ("min_completion_chars", 0, 2**63 - 1)]
    for name, lower, upper in limits:
        value = getattr(config, name)
        if type(value) is not int or not lower <= value <= upper:
            raise ValueError(f"{name} must be an integer between {lower} and {upper}")
    if config.max_tokens is not None and (type(config.max_tokens) is not int or config.max_tokens <= 0):
        raise ValueError("max_tokens must be positive")
    if config.max_tokens is not None and not config.tokenizer_dir:
        raise ValueError("max_tokens requires a local tokenizer directory; token counts are never guessed")
    if config.normalization not in {"exact", "whitespace"}:
        raise ValueError("normalization must be exact or whitespace")
    if config.near_threshold is not None and not (math.isfinite(config.near_threshold) and 0 < config.near_threshold <= 1):
        raise ValueError("near_threshold must be finite and greater than zero, at most one")
