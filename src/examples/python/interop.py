# osprey: flavor=python
# Osprey source: run with Osprey's compiler or Python's osprey.build, not CPython.

@extern
def osprey_python_call(module: str, name: str, args: str) -> str:
    ...


@extern
def osprey_python_object(operation: str, object_id: int, name: str, args: str) -> str:
    ...


def greeting(name: str):
    return f"hi {name}"


def square_root_json(value: float):
    return osprey_python_call("math", "sqrt", f"[{value}]")


def scale_tensor_json(object_id):
    return osprey_python_object("call", object_id, "mul", '{"args":[{"value":2.0}]}')
