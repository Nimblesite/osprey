# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=Inference
"""The one inference seam between the routes and whatever model is installed.

A route never names a weight file, a device or a vocabulary: it hands the
installed model's handle to this effect, which tokenizes a prompt, takes one
sampling step at a time and decodes each token. A step answers with the token
and the handle to continue from, so a handler that caches (the real weights'
KV and recurrent state, `RealModel.py`) carries its advance in the handle and
the turn loop threads it, while the fixture table (`TableModel.py`) hands the
same handle back ([LLM-SERVING], [LLM-MODEL-CACHE]).
"""

from qwen_server import Requests as Q

__all__ = ["Stepped", "Generator"]


class Stepped[M]:
    """One sampled token and the handle to take the next step from."""
    token: int
    model: M


@effect
class Generator[M]:
    """Tokenize in, tokenize out, and one sampling step over a token history."""

    def encode(model: M, text: str) -> Result[list[int], Error]: ...
    def decode(model: M, token: int) -> Result[str, Error]: ...
    def eos(model: M) -> int: ...
    def step(model: M, history: list[int], sampling: Q.Sampling) -> Result[Stepped[M], Error]: ...
