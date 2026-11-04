"""Generate Osprey records and unions from typeDiagram, retaining field types."""
import ast
import subprocess
from pathlib import Path

ROOT = Path(__file__).parent

TYPE_DOCS = {
    "Request": "Incoming HTTP request with raw URL/body and decoded path captures.",
    "Response": "Outgoing HTTP status, content, media type and headers.",
    "Route": "An ordered method/path pattern and its typed handler.",
    "Decoded": "One decoded URL fragment and the unconsumed input.",
    "Parameter": "One decoded query name/value pair.",
    "HTTPException": "A typed HTTP failure with status, detail and headers.",
    "ValidationIssue": "A validation failure with location, original input and typed context.",
    "Pair": "Two independently typed validated values.",
    "Validation": "A validated value or accumulated request validation issues.",
    "ValidationContext": "Typed optional metadata for a validation issue.",
}


def variants(value, prefix):
    if isinstance(value, ast.Name):
        return [value.id]
    if isinstance(value, ast.BinOp) and isinstance(value.op, ast.BitOr):
        return variants(value.left, prefix) + variants(value.right, prefix)
    if isinstance(value, ast.Constant) and isinstance(value.value, str):
        return [prefix + value.value]
    raise ValueError(f"Unexpected union: {ast.dump(value)}")


def declarations(source):
    nodes = ast.parse(source).body
    classes = {node.name: node for node in nodes if isinstance(node, ast.ClassDef)}
    unions = {node.targets[0].id: variants(node.value, node.targets[0].id) for node in nodes
              if isinstance(node, ast.Assign) and isinstance(node.value, ast.BinOp)}
    members = {name for names in unions.values() for name in names}
    for name in members:
        if name not in classes:
            classes[name] = ast.ClassDef(name=name, bases=[], keywords=[], body=[ast.Pass()], decorator_list=[])
    records = [node for name, node in classes.items() if name not in members]
    return classes, unions, records


def fields(node, indent):
    output = []
    for field in node.body:
        if isinstance(field, ast.Pass):
            continue
        if not isinstance(field, ast.AnnAssign) or not isinstance(field.target, ast.Name):
            raise ValueError(f"Unexpected typeDiagram field: {ast.dump(field)}")
        output.append(f"{indent}{field.target.id}: {ast.unparse(field.annotation)}")
    return output


def parameters(node):
    if not node.bases:
        return ""
    generic = node.bases[0]
    if not isinstance(generic, ast.Subscript):
        raise ValueError(f"Unexpected base: {ast.dump(generic)}")
    parts = generic.slice.elts if isinstance(generic.slice, ast.Tuple) else [generic.slice]
    return "[" + ", ".join(ast.unparse(part) for part in parts) + "]"


def records(nodes):
    output = []
    for node in nodes:
        generic = parameters(node)
        output.extend(["", "", f"class {node.name}{generic}:", '    """' + TYPE_DOCS[node.name] + '"""'] + fields(node, "    "))
    return output


def union_records(classes, unions):
    output = []
    for name, members in unions.items():
        generic = next((parameters(classes[member]) for member in members if classes[member].bases), "")
        output.extend(["", "", "@union", f"class {name}{generic}:", '    """' + TYPE_DOCS[name] + '"""'])
        for member in members:
            if not member.startswith(name):
                raise ValueError(f"Unexpected variant name: {member}")
            output.extend([f"    class {member[len(name):]}:"] + (fields(classes[member], "        ") or ["        pass"]))
    return output


def generate():
    source = subprocess.check_output(
        ["typediagram", "--to", "python", str(ROOT / "models.td")], text=True
    )
    classes, unions, nodes = declarations(source)
    output = ['# osprey: flavor=python', '# osprey: namespace=fastapi',
              '# osprey: module=Models', '"""Generated from models.td; run generate_models.py."""']
    output.extend(["", "__all__ = " + repr([node.name for node in nodes] + list(unions))])
    output.extend(records(nodes) + union_records(classes, unions))
    (ROOT / "src" / "models.py").write_text("\n".join(output) + "\n")


if __name__ == "__main__":
    generate()
