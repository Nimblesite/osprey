# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=Qwen3_5Rope
"""`Qwen3_5TextRotaryEmbedding`: the plain 1-D rotary tables and the rotation of a head.

The inverse frequencies are `1 / rope_theta ** (2j / dim)` over `dim = head_dim *
partial_rotary_factor`, binary32 through `pow` — 64 of the real model's 256 head
dimensions, and 16 * 0.25 = 4 in the fixture. The text path expands one `position_ids`
row onto the model's three position axes, so the source's `recomposition_frequencies`
writes each axis onto itself and the table is the frequency row concatenated with
itself; the fixture's `rotary_emb` hook confirms that reduction. `rotate` turns only
the first `len(cos)` values of a head vector and passes the rest through, exactly as
`apply_rotary_pos_emb`'s `q_rot`/`q_pass` split does. Implements [LLM-MODEL-ROPE].
"""

__all__ = ["Table", "default_dim", "inv_freq", "tables", "rotate", "rotate_half_at"]

from transformers.Qwen3_5Tensor import at, then


class Table:
    """`cos` and `sin` for one position per row, each `dim` values wide: the frequency
    row doubled, so the rotation's `rotate_half` pairs line up."""
    cos: list[list[float32]]
    sin: list[list[float32]]


def default_dim(config):
    """`int(head_dim * partial_rotary_factor)`, the source's rotary width, or an `Error`
    when the product is not a finite integer."""
    match floatToInt(toFloat32(config.head_dim) * config.partial_rotary_factor):
        case Success(value=dim):
            return Success(value=dim)
        case Error(message=failure):
            return Error(message="rotary dimension: " + failure)


def inv_freq(dim, theta):
    """`1 / theta ** (2j / dim)` for j in [0, dim/2), in binary32."""
    return toList(map(range(dim // 2), lambda j: toFloat32(1.0) / pow(theta, toFloat32(2 * j) / toFloat32(dim))))


def tables(start, count, config):
    """The tables for positions `start ..< start + count`."""
    return then(default_dim(config), lambda dim: Success(value=tables_of(start, count, dim, config.rope_theta)))


def tables_of(start, count, dim, theta) -> Table:
    """One row per position, each row doubled from its frequencies."""
    freqs = inv_freq(dim, theta)
    return Table(cos=toList(map(range(count), lambda t: doubled(freqs, toFloat32(start + t), cos))), sin=toList(map(range(count), lambda t: doubled(freqs, toFloat32(start + t), sin))))


def doubled(freqs, position: float32, of):
    """`of(position * frequency)` for every frequency, the row repeated once."""
    half = mapList(freqs, lambda frequency: of(position * frequency))
    return listConcat(half, half)


def rotate(v, cos, sin):
    """`apply_rotary_pos_emb` on one head vector: `v_rot * cos + rotate_half(v_rot) * sin`
    over the first `len(cos)` values, then the untouched tail."""
    turned = toList(map(range(len(cos)), lambda i: at(v, i) * at(cos, i) + rotate_half_at(v, len(cos) // 2, i) * at(sin, i)))
    return listConcat(turned, toList(map(range(len(v) - len(cos)), lambda i: at(v, len(cos) + i))))


def rotate_half_at(v, half, i) -> float32:
    """`rotate_half(v)[i]` over a rotary slice of `2 * half` values: `-v[i + half]` in the
    first half and `v[i - half]` in the second."""
    if i < half:
        return toFloat32(0.0) - at(v, i + half)
    else:
        return at(v, i - half)
