# osprey: flavor=python
# Deliberately planted Python-flavor crash [SYMEX-CRASH-GATE].
@extern
def abort() -> int:
    ...

result = abort() if input() == "Y" else 0
print(f"result {result}")
