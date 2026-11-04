"""Run from the repo root after installing src/toolchain/python/: python src/osprey/examples/python/run.py."""

import json
from pathlib import Path
import osprey


def main():
    library = osprey.build(Path(__file__).with_name("interop.osp"))
    assert library.squareRootText(16.0) == "4.0"
    assert library.mockedSquareRoot() == "test answer"
    print(library.squareRootText(-1.0))
    python_flavor = osprey.build(Path(__file__).with_name("interop.py"))
    assert json.loads(python_flavor.square_root_json(25.0)) == {"ok": 5.0}
    tensor_example(library)


def tensor_example(library):
    # Optional real tensor use. This example never installs packages or selects a GPU.
    try:
        import torch
    except ImportError:
        print("Scalar/effect calls passed; install PyTorch to try the tensor handle call.")
        return
    original = torch.tensor([1.0, 2.0, 3.0])
    source = library.keep(original)
    result = json.loads(library.scaleTensor(source))
    if "error" in result:
        library.release(source)
        raise RuntimeError(result["error"])
    output = result["ok"]
    print(library.resolve(output))
    library.release(output)
    library.release(source)


if __name__ == "__main__":
    main()
