"""Run the native parser against real checked-in ML programs."""
from pathlib import Path
import subprocess
import sys

GRAMMAR = Path(__file__).resolve().parents[1]
REPOSITORY = GRAMMAR.parent
CLI = REPOSITORY / "tree-sitter-osprey/node_modules/.bin/tree-sitter"
ROOTS = ("tests", "examples")
EXCLUDED = {"failscompilation", "node_modules", "target"}
OUTPUT = Path(sys.argv[1]).resolve()
FILES = sorted(path for root in ROOTS for path in (REPOSITORY / root).rglob("*.ospml")
               if not EXCLUDED.intersection(path.parts))

result = subprocess.run([str(CLI), "parse", "--quiet", "--stat", *(str(path) for path in FILES)],
                        cwd=GRAMMAR, capture_output=True, text=True, check=False)
OUTPUT.parent.mkdir(parents=True, exist_ok=True)
OUTPUT.write_text("# Native Osprey ML corpus\n\n```text\n"
                  + result.stdout + result.stderr + "```\n")
print(f"Native ML corpus exit {result.returncode}; {OUTPUT}")
sys.exit(result.returncode)
