"""The word exists only in the MCP server's environment, so reaching this file proves the model used the server."""
import os
from pathlib import Path
import sys

written = (Path(sys.argv[1]) / "WORD.txt").read_text(encoding="utf-8").strip()
if written != os.environ["MCP_LIVE_WORD"]:
    raise SystemExit(f"mcp-word: WORD.txt holds {written!r}")
print("mcp-word: the model read the word through its MCP server")
