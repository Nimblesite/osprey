# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=Lists
"""One sequence helper, for a fault the runtime has and the server works around.

`length` on a list that crossed a handler return inside the `httpListen`
callback reports 1, while `listGet`, `mapList` and `foldList` read every
element (reproduced in `target/local-coding-model/pi-server/repro`: a four-token
list answers `len` 1 and a folded sum 10). The fault is reported to the plan's
lead; until it is fixed the server counts its token and call lists by folding,
so a prompt is never mis-measured and a context limit is never checked against a
length the runtime got wrong.
"""

__all__ = ["count"]


def count(values):
    """The element count of `values`, read from its elements rather than its header."""
    return foldList(values, 0, lambda counted, _value: satAdd(counted, 1))
