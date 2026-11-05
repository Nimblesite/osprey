"""Writes the tensor table of the real GGUF as Python-flavor Osprey, so qwen.py names tensors
as the file does. Run with the oracle venv: `table_gen.py <gguf> "[ids]" > tensors.py` (not committed)."""
import sys
import gguf

reader = gguf.GGUFReader(sys.argv[1])
print("# osprey: flavor=python")
print(f'GGUF = "{sys.argv[1]}"')
print("class Entry:\n    name: str\n    offset: int\n    size: int\n    kind: str\n")
print("def entries():\n    return [")
for t in reader.tensors:
    print(f'        Entry(name="{t.name}", offset={t.data_offset}, size={t.n_bytes}, kind="{t.tensor_type.name}"),')
print("    ]")
embed = next(t for t in reader.tensors if t.name == "token_embd.weight")
print(f"EMBED_AT = {embed.data_offset}\nEMBED_ROW = {embed.n_bytes // int(embed.shape[1])}\nVOCAB = {int(embed.shape[1])}")
print(f"PROMPT = {sys.argv[2]}")
