"""Compile once, audit in bounded native workers, publish only a complete result."""

import os
from pathlib import Path
import subprocess
import sys

import osprey
from .config import validate
from .output import destination
from . import report, store, tokenizer


def run_audit(config):
    config = validate(config)
    with destination(config.output) as staging:
        return _run(config, staging)


def _run(config, staging):
    database = staging / "audit.sqlite3"
    connection = store.create(database, config)
    try:
        sources = store.ingest(connection, config)
        if not sum(source["records"] for source in sources if source["split"] == "train"):
            raise ValueError("Training inputs contain no records")
        metadata = tokenizer.metadata(config.tokenizer_dir)
        _validate_chat_template(connection, config)
        library = _compile(staging, config)
        processes = _workers(connection, database, library, config)
        result = report.export(connection, staging, config, sources, metadata, processes)
    finally:
        connection.close()
    _clean_work(staging, config)
    return result


def _validate_chat_template(connection, config):
    if config.tokenizer_dir and connection.execute("SELECT 1 FROM records WHERE kind='chat' LIMIT 1").fetchone():
        if not tokenizer.load(config.tokenizer_dir).chat_template:
            raise ValueError("Chat data requires a local tokenizer with its model chat template")


def _compile(staging, config):
    suffix = ".dylib" if sys.platform == "darwin" else ".so"
    path = staging / ("policy" + suffix)
    library = osprey.build(Path(__file__).with_name("policy.py"), path, compiler=config.compiler)
    if library.policy_version() != 1:
        raise RuntimeError("Unsupported native audit policy version")
    return path


def _workers(connection, database, library, config):
    total = connection.execute("SELECT COUNT(*) FROM records").fetchone()[0]
    environment = dict(os.environ, PYTHONPATH=os.pathsep.join(sys.path))
    processes = 0
    for start in range(1, total + 1, config.batch_size):
        count = min(config.batch_size, total - start + 1)
        result = subprocess.run([sys.executable, "-m", "osprey.dataset_audit.worker", str(database), str(library), str(start), str(count)],
                                capture_output=True, text=True, env=environment, timeout=config.worker_timeout)
        if result.returncode:
            raise RuntimeError(result.stderr.strip() or "The native audit worker failed")
        processes += 1
    return processes


def _clean_work(staging, config):
    for path in staging.glob("policy.*"):
        path.unlink()
    if not config.keep_database:
        (staging / "audit.sqlite3").unlink()
