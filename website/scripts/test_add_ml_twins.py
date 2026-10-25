#!/usr/bin/env python3
# Tests for add-ml-twins.py translation rules.
# Run: python3 scripts/test_add_ml_twins.py   (from website/)
import importlib.util
import os
import pathlib
import re
import subprocess
import tempfile

_HERE = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("add_ml_twins", _HERE / "add-ml-twins.py")
add_ml_twins = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(add_ml_twins)

BUILTIN_CASES = {
    "checkedadd": ("t", "0\n"),
    "checkedsub": ("d", "6\n"),
    "checkedmul": ("p", "42\n"),
    "indexof": (None, "3\n"),
    "lines": (None, "a,b,c\n"),
    "words": (None, "a,b,c\n"),
    "structuralequals": ('"${same} ${differ}"', "true false\n"),
    "jsonparse": (None, "name=Success(osprey); items=2; released=Success(0)\n"),
    "spawnprocess": (None, "1: hello\n3: 0\nexit: 0\n"),
}


def verify_builtin_pair(compiler, root, name, probe, expected):
    page = root / "functions" / f"{name}.md"
    add_ml_twins.process(page, add_ml_twins.HAND_TWINS.get(f"functions/{name}.md"))
    blocks = re.findall(r"^```(osprey(?:-ml)?)\n(.*?)^```", page.read_text(), re.M | re.S)
    assert len(blocks) == 2, f"missing flavor pair for {name}"
    for flavor, source in blocks:
        extension = "ospml" if flavor == "osprey-ml" else "osp"
        path = root / f"{name}.{extension}"
        suffix = f"\nprint({probe})\n" if probe else ""
        path.write_text(source + suffix)
        result = subprocess.run([str(compiler), str(path), "--run"], capture_output=True, text=True, timeout=30)
        assert result.returncode == 0, f"{path.name}: {result.stderr}"
        assert not result.stderr, f"{path.name}: {result.stderr}"
        assert result.stdout == expected, f"{path.name}: {result.stdout!r} != {expected!r}"


def test_generated_builtin_examples_execute_in_both_flavors():
    default = _HERE.parents[1] / "target" / "release" / "osprey"
    compiler = pathlib.Path(os.environ.get("OSPREY_BIN", default)).resolve()
    with tempfile.TemporaryDirectory(prefix="osprey-builtin-examples-") as directory:
        root = pathlib.Path(directory)
        subprocess.run([str(compiler), "--docs", "--docs-dir", str(root)], check=True, capture_output=True, text=True)
        for name, (probe, expected) in BUILTIN_CASES.items():
            verify_builtin_pair(compiler, root, name, probe, expected)


def test_match_arm_brace_pattern_binds_by_juxtaposition():
    """A `match` arm whose pattern carries a brace payload — `Success { value }`
    — must lower to ML juxtaposition `Success value`. Braces in a match pattern
    are invalid ML (spec 0024 [FLAVOR-ML-MATCH]; crates/osprey-syntax/src/ml/cst.rs).
    """
    default = (
        "match byteAt(\"hi\", 0) {\n"
        "    Success { value } => print(\"byte: ${value}\")\n"
        "    Error { message } => print(message)\n"
        "}"
    )
    ml = add_ml_twins.translate_block(default)
    assert ml is not None, "expected a translated ML twin"
    assert "Success value =>" in ml, f"pattern not juxtaposed:\n{ml}"
    assert "Error message =>" in ml, f"pattern not juxtaposed:\n{ml}"
    # No brace payload may survive in an arm *pattern* (before the `=>`).
    # String-interpolation braces `${...}` in an arm *body* are valid ML and OK.
    for arm in ml.splitlines():
        pat = arm.split("=>", 1)[0]
        assert "{" not in pat and "}" not in pat, f"braces leaked into ML pattern:\n{arm}"


def test_match_preserves_prerequisite_comment():
    source = '// Supply weights.bin before running.\nmatch gpuLoad("weights.bin", 0, 3) {\n    Success { value } => print(gpuLength(value))\n    Error { message } => print(message)\n}'
    ml = add_ml_twins.translate_block(source)
    assert ml.startswith('// Supply weights.bin before running.\nmatch gpuLoad '), ml


if __name__ == "__main__":
    test_match_preserves_prerequisite_comment()
    test_match_arm_brace_pattern_binds_by_juxtaposition()
    print("PASS: match-arm brace pattern binds by juxtaposition")
    test_generated_builtin_examples_execute_in_both_flavors()
    print("PASS: generated builtin examples execute in both flavors")
