# osprey: flavor=python
# osprey: namespace=einops
# osprey: module=Reconstruct
"""The shape-inference resolution: a `Recipe.TransformRecipe` against a concrete
input shape into concrete reshapes, matching
`einops.einops._reconstruct_from_shape` (einops 0.8.2).

Kept in its own module so `Recipe` holds the pattern parsing/agreement and
`Reconstruct` holds the shape resolution. Errors are `Result` with the
original's `EinopsError` message text.
"""
from einops.Recipe import NamePos, PosElem, TransformRecipe, set_at

__all__ = ["PosLen", "NameLen", "CookedRecipe", "reconstruct"]

class PosLen:
    """An added axis resolved to a length: output position `pos`, length `length`."""
    pos: int
    length: int


class NameLen:
    """A caller-supplied axis length: axis `name`, length `length`."""
    name: str
    length: int


class CookedRecipe:
    """A recipe resolved against one input shape. An empty `init_shapes`,
    `axes_reordering` or `final_shapes` means the step is absent (the original's
    `None`)."""
    init_shapes: list[int]
    axes_reordering: list[int]
    reduced_axes: list[int]
    added_axes: list[PosLen]
    final_shapes: list[int]
    n_axes_after_adding_axes: int



def take_list(xs, n) -> list[int]:
    """The first `n` elements of `xs`."""
    return toList(map(range(n), lambda i: unwrap_or(listGet(xs, i), 0)))



def lookup_name(name2pos, name) -> int:
    """The elementary position a supplied axis name maps to, or -1."""
    return fold(range(len(name2pos)), -1, lambda found, j: unwrap_or(listGet(name2pos, j), NamePos(name="", pos=0)).pos if found == -1 and unwrap_or(listGet(name2pos, j), NamePos(name="", pos=0)).name == name else found)


def bind_axes_go(axes_lengths, name2pos, lengths, i) -> list[int]:
    if i >= len(axes_lengths):
        return lengths
    else:
        entry = unwrap_or(listGet(axes_lengths, i), NameLen(name="", length=0))
        pos = lookup_name(name2pos, entry.name)
        return bind_axes_go(axes_lengths, name2pos, set_at(lengths, pos, entry.length), i + 1)


def reconstruct(recipe, shape, axes_lengths) -> Result[CookedRecipe, str]:
    """Resolve a recipe against a concrete shape into concrete reshapes."""
    lengths = bind_axes_go(axes_lengths, recipe.axis_name2elementary_axis, recipe.elementary_axes_lengths, 0)
    return reconstruct_body(recipe, shape, lengths, 0, False)


def reconstruct_body(recipe, shape, lengths, input_axis, need_init_reshape) -> Result[CookedRecipe, str]:
    """Infer each input group's unknown axis, checking the shape as it goes."""
    if input_axis >= len(recipe.input_composition_known_unknown):
        return Success(value=cook(recipe, lengths, need_init_reshape))
    else:
        return reconstruct_entry(recipe, shape, lengths, input_axis, need_init_reshape)


def reconstruct_entry(recipe, shape, lengths, input_axis, need_init_reshape) -> Result[CookedRecipe, str]:
    entry = unwrap_or(listGet(recipe.input_composition_known_unknown, input_axis), [[], []])
    known_axes = unwrap_or(listGet(entry, 0), [])
    unknown_axes = unwrap_or(listGet(entry, 1), [])
    dim = unwrap_or(listGet(shape, input_axis), 0)
    need_after = need_init_reshape or (len(known_axes) + len(unknown_axes) != 1)
    if len(known_axes) == 0 and len(unknown_axes) == 1:
        return reconstruct_body(recipe, shape, set_at(lengths, unwrap_or(listGet(unknown_axes, 0), 0), dim), input_axis + 1, need_after)
    else:
        return reconstruct_product(recipe, shape, lengths, input_axis, need_after, known_axes, unknown_axes, dim)


def reconstruct_product(recipe, shape, lengths, input_axis, need_after, known_axes, unknown_axes, dim) -> Result[CookedRecipe, str]:
    known_product = foldList(toList(map(range(len(known_axes)), lambda i: unwrap_or(listGet(lengths, unwrap_or(listGet(known_axes, i), 0)), 0))), 1, lambda a, x: a * x)
    return reconstruct_check(recipe, shape, lengths, input_axis, need_after, unknown_axes, dim, known_product)


def reconstruct_check(recipe, shape, lengths, input_axis, need_after, unknown_axes, dim, known_product) -> Result[CookedRecipe, str]:
    if len(unknown_axes) == 0:
        return reconstruct_all_known(recipe, shape, lengths, input_axis, need_after, dim, known_product)
    elif dim % known_product != 0:
        return Error(message=f"Shape mismatch, can't divide axis of length {dim} in chunks of {known_product}")
    else:
        return reconstruct_body(recipe, shape, set_at(lengths, unwrap_or(listGet(unknown_axes, 0), 0), dim // known_product), input_axis + 1, need_after)


def reconstruct_all_known(recipe, shape, lengths, input_axis, need_after, dim, known_product) -> Result[CookedRecipe, str]:
    if dim != known_product:
        return Error(message=f"Shape mismatch, {dim} != {known_product}")
    else:
        return reconstruct_body(recipe, shape, lengths, input_axis + 1, need_after)


def identity_perm_go(perm, i) -> bool:
    if i >= len(perm):
        return True
    else:
        return unwrap_or(listGet(perm, i), 0) == i and identity_perm_go(perm, i + 1)


def identity_perm(perm) -> bool:
    return identity_perm_go(perm, 0)


def cook(recipe, lengths, need_init_reshape) -> CookedRecipe:
    """The concrete reshapes from fully inferred elementary lengths."""
    n_perm = len(recipe.axes_permutation)
    init_shapes = take_list(lengths, n_perm) if need_init_reshape else []
    final_shapes = toList(map(range(len(recipe.output_composite_axes)), lambda g: foldList(unwrap_or(listGet(recipe.output_composite_axes, g), []), 1, lambda acc, pos: acc * unwrap_or(listGet(lengths, pos), 0))))
    need_final = len(filterList(recipe.output_composite_axes, lambda g: len(g) != 1)) > 0
    added = toList(map(range(len(recipe.added_axes)), lambda i: PosLen(pos=unwrap_or(listGet(recipe.added_axes, i), PosElem(pos=0, elementary=0)).pos, length=unwrap_or(listGet(lengths, unwrap_or(listGet(recipe.added_axes, i), PosElem(pos=0, elementary=0)).elementary), 0))))
    reduced = toList(map(range(recipe.first_reduced_axis, n_perm), lambda i: i))
    reordering = [] if identity_perm(recipe.axes_permutation) else recipe.axes_permutation
    return CookedRecipe(
        init_shapes=init_shapes,
        axes_reordering=reordering,
        reduced_axes=reduced,
        added_axes=added,
        final_shapes=final_shapes if need_final else [],
        n_axes_after_adding_axes=len(added) + n_perm,
    )

