# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=Requests
"""The served model's declared shape and the records the wire layer builds.

The metadata here is the model plan 0049 pins
(docs/specs/applications/LocalCodingModel.md *Model and weights* and
*Memory budget*): `Qwen/Qwen3.8-27B` at revision `1d4bf0f2…` with weights from
`unsloth/Qwen3.8-27B-GGUF` revision `4ca7207…`, quantized `UD-Q4_K_XL` at 64K
context. The inference handler behind these endpoints is the fixture model in
`TableModel.py` until plan 0049 Phase 3 lands the real weights; the declared
shape is what the weights *will* be, and no field below claims a byte the
server has read. `digest` is therefore absent rather than invented.

No clock builtin exists in Osprey ([BUILTIN-TERM] has none), so the timestamps
the protocols require are one fixed instant rather than a measured one.
"""

from qwen_server import Wire

__all__ = [
    "MODEL", "FAMILY", "QUANTIZATION", "CONTEXT", "BLOCKS", "EMBEDDING",
    "PARAMETERS", "WEIGHTS_BYTES", "WEIGHT_FILE", "REVISION", "INSTANT",
    "VERSION", "TEMPERATURE", "show_request",
]

MODEL = "qwen3.8:27b"
FAMILY = "qwen3_5"
QUANTIZATION = "UD-Q4_K_XL"
CONTEXT = 65536
BLOCKS = 64
EMBEDDING = 5120
PARAMETERS = 27781427952
WEIGHTS_BYTES = 18897914266
WEIGHT_FILE = "Qwen3.8-27B-UD-Q4_K_XL.gguf"
REVISION = "4ca720788d1e01f1bff70c033e0d0028fd02e502"
INSTANT = "2026-11-05T00:00:00Z"
VERSION = "0.1.0"
TEMPERATURE = 0.0


def show_request(body):
    """The model name from an `/api/show` body ([LLM-API-OLLAMA])."""
    match Wire.decode(body):
        case Success(value=document):
            return required(Wire.text(document, "model"))
        case Error(message=message):
            return Error(message=message)


def required(name):
    """A required model name, or the rejection both protocols use."""
    return Success(value=name) if name != "" else Error(message="model is required")
