"""A local fine-tuning data quality gate: keep useful records and explain every rejection."""

import argparse
from pathlib import Path
import sys

from .config import Config
from .runner import run_audit


def arguments(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--train", action="append", type=Path, required=True)
    parser.add_argument("--eval", dest="evaluation", action="append", type=Path, default=[])
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--compiler")
    parser.add_argument("--batch-size", type=int, default=2000)
    parser.add_argument("--worker-timeout", type=int, default=300)
    parser.add_argument("--max-line-bytes", type=int, default=1024 * 1024)
    parser.add_argument("--max-chars", type=int, default=32000)
    parser.add_argument("--min-completion-chars", type=int, default=1)
    return _more_arguments(parser, argv)


def _more_arguments(parser, argv):
    parser.add_argument("--tokenizer-dir", type=Path)
    parser.add_argument("--max-tokens", type=int)
    parser.add_argument("--separator", default="\n\n")
    parser.add_argument("--near-threshold", type=float)
    parser.add_argument("--normalization", choices=["exact", "whitespace"], default="whitespace")
    parser.add_argument("--allow-personal-data", action="store_true")
    parser.add_argument("--keep-database", action="store_true")
    parser.add_argument("--allow-quarantine", action="store_true")
    return parser.parse_args(argv)


def main(argv=None):
    options = vars(arguments(argv))
    allow = options.pop("allow_quarantine")
    try:
        result = run_audit(Config(**options))
        summary = result["summary"]
        print(f'Audited {summary["records"]} records: {summary["kept"]} kept, {summary["quarantined"]} quarantined. Output: {options["output"]}')
        return 2 if summary["quarantined"] and not allow else 0
    except Exception as error:
        print(f"Dataset audit failed: {error}", file=sys.stderr)
        return 1
