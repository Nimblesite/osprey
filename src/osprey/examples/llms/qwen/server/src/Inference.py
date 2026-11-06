# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=Inference
"""The one inference seam between the routes and whatever model is installed.

A route never names a weight file, a device or a vocabulary: it hands the
installed model's handle to this effect, which tokenizes a prompt, takes one
sampling step at a time and decodes each token. Plan 0049 Phase 3 puts the
`qwen3_5` weights behind this effect — the handle becomes a mapped-weights and
KV-state session — and no route changes ([LLM-SERVING], [LLM-STATE-OWNED]).
"""

from qwen_server import Requests as Q

__all__ = ["Generator"]


@effect
class Generator[M]:
    """Tokenize in, tokenize out, and one sampling step over a token history."""

    def encode(model: M, text: str) -> Result[list[int], Error]: ...
    def decode(model: M, token: int) -> Result[str, Error]: ...
    def eos(model: M) -> int: ...
    def step(model: M, history: list[int], sampling: Q.Sampling) -> Result[int, Error]: ...
