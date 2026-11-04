"""Generate Osprey records from typeDiagram; fail if unexpected constructs appear."""
import ast
import subprocess
from pathlib import Path

ROOT = Path(__file__).parent


def generate():
    source = subprocess.check_output(
        ["typediagram", "--to", "python", str(ROOT / "models.td")], text=True
    )
    classes = [node for node in ast.parse(source).body if isinstance(node, ast.ClassDef)]
    output = ['# osprey: flavor=python', '# osprey: namespace=fastapi',
              '# osprey: module=Models', '"""Generated from models.td; run generate_models.py."""']
    output.extend(["", "__all__ = " + repr([node.name for node in classes])])
    output.extend(records(classes))
    (ROOT / "src" / "models.py").write_text("\n".join(output) + "\n")


def records(classes):
    output = []
    for node in classes:
        generic = "[T]" if node.bases else ""
        output.extend(["", "", f"class {node.name}{generic}:"])
        for field in node.body:
            if not isinstance(field, ast.AnnAssign) or not isinstance(field.target, ast.Name):
                raise ValueError(f"Unexpected typeDiagram field: {ast.dump(field)}")
            output.append(f"    {field.target.id}: {ast.unparse(field.annotation)}")
    return output


if __name__ == "__main__":
    generate()
