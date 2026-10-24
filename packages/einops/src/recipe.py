# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=einops
# osprey: module=Recipe
"""The shape-inference recipe behind `rearrange`, `reduce` and `repeat`.

`prepare` turns a pattern into a pure `TransformRecipe` (elementary axis
lengths, the permutation, the reduced axes, the added axes and the final
reshape grouping), and `reconstruct` resolves that recipe against a concrete
input shape into a `CookedRecipe` of concrete reshapes, matching
`einops.einops._prepare_transformation_recipe` and
`einops.einops._reconstruct_from_shape` (einops 0.8.2). Magic axis lengths use
the original's markers: `-999999` unknown, `-99999` supplied by the caller.
"""
from einops.Parsing import ELLIPSIS, Axis, Composite, axis_in_idents, check_axis_name, format_axes, named, parse

__all__ = ["TransformRecipe", "NamePos", "PosElem", "prepare", "set_at"]

UNKNOWN_AXIS_LENGTH = -999999
EXPECTED_AXIS_LENGTH = -99999
REDUCTION_NAMES = ["min", "max", "sum", "mean", "prod", "any", "all"]
REDUCTION_TEXT = "('min', 'max', 'sum', 'mean', 'prod', 'any', 'all')"


class NamePos:
    """An axis name bound to its elementary position."""
    name: str
    pos: int


class PosElem:
    """An added axis: output position `pos`, elementary position `elementary`."""
    pos: int
    elementary: int


class GroupAssigned:
    """One group's assignment: the grown elementary axes and its positions."""
    axes: list[Axis]
    lengths: list[int]
    positions: list[int]


class SideAssigned:
    """A whole side's assignment: elementary axes, lengths and per-group positions."""
    axes: list[Axis]
    lengths: list[int]
    positions: list[list[int]]


class TransformRecipe:
    """The pure computation pathway a pattern describes."""
    elementary_axes_lengths: list[int]
    axis_name2elementary_axis: list[NamePos]
    input_composition_known_unknown: list[list[list[int]]]
    axes_permutation: list[int]
    first_reduced_axis: int
    added_axes: list[PosElem]
    output_composite_axes: list[list[int]]


def is_reduction(operation) -> bool:
    """Whether `operation` names one of the built-in reductions."""
    return listContains(REDUCTION_NAMES, operation)


def flatten(xss):
    return foldList(xss, [], lambda acc, xs: listConcat(acc, xs))


def list_index_go(xs, v, i, found) -> int:
    if i >= len(xs):
        return found
    elif found == -1 and unwrap_or(listGet(xs, i), 0) == v:
        return i
    else:
        return list_index_go(xs, v, i + 1, found)


def list_index(xs, v) -> int:
    """The first index of `v` in `xs`, or -1 when absent."""
    return list_index_go(xs, v, 0, -1)


def set_at(xs, i, v) -> list[int]:
    """`xs` with element `i` replaced by `v`; shared with `Reconstruct`."""
    return toList(map(range(len(xs)), lambda j: v if j == i else unwrap_or(listGet(xs, j), v)))


def find_named(axes, name) -> int:
    """The elementary position of a named axis, or -1."""
    return fold(range(len(axes)), -1, lambda found, j: j if found == -1 and (lambda a: a.kind == "named" and a.name == name)(unwrap_or(listGet(axes, j), named(""))) else found)


def set_difference(a, b) -> list[Axis]:
    """`a` minus `b` over identifier lists."""
    return filterList(a, lambda ax: not axis_in_idents(b, ax))


def symmetric_difference(a, b) -> list[Axis]:
    """Elements of `a` or `b` but not both."""
    return listConcat(set_difference(a, b), set_difference(b, a))


def unpack_error(n) -> str:
    """Python's unpacking message for a pattern without exactly one `->`."""
    if n < 2:
        return f"not enough values to unpack (expected 2, got {n})"
    else:
        return f"too many values to unpack (expected 2, got {n})"


def composition_axes(expr) -> list[list[Axis]]:
    """A parsed composition as plain groups of axes."""
    return toList(map(range(len(expr.composition)), lambda i: unwrap_or(listGet(expr.composition, i), Composite(kind="group", axes=[])).axes))


def expand_left_comp(comp, ell_axes) -> list[list[Axis]]:
    """The left composition with the ellipsis slot expanded to one group per axis."""
    return foldList(comp, [], lambda acc, c: listConcat(acc, toList(map(range(len(ell_axes)), lambda i: [unwrap_or(listGet(ell_axes, i), named(""))]))) if c.kind == "ellipsis" else listAppend(acc, c.axes))


def expand_right_one(c, ell_axes) -> list[list[Axis]]:
    if c.kind == "ellipsis":
        return toList(map(range(len(ell_axes)), lambda i: [unwrap_or(listGet(ell_axes, i), named(""))]))
    else:
        group = foldList(c.axes, [], lambda acc, ax: listConcat(acc, ell_axes) if ax.kind == "ellipsis" else listAppend(acc, ax))
        return [group]


def expand_right_comp(comp, ell_axes) -> list[list[Axis]]:
    """The right composition with every ellipsis marker expanded to the ellipsis axes."""
    return flatten(toList(map(range(len(comp)), lambda i: expand_right_one(unwrap_or(listGet(comp, i), Composite(kind="group", axes=[])), ell_axes))))


def assign_group_go(group, axes, lengths, j, positions) -> GroupAssigned:
    if j >= len(group):
        return GroupAssigned(axes=axes, lengths=lengths, positions=positions)
    else:
        return assign_group_axis(group, axes, lengths, j, positions, unwrap_or(listGet(group, j), named("")))


def assign_group_axis(group, axes, lengths, j, positions, axis) -> GroupAssigned:
    if axis.kind == "named":
        return assign_named_axis(group, axes, lengths, j, positions, axis)
    else:
        return assign_group_go(group, listAppend(axes, axis), listAppend(lengths, axis.value), j + 1, listAppend(positions, len(axes)))


def assign_named_axis(group, axes, lengths, j, positions, axis) -> GroupAssigned:
    pos = find_named(axes, axis.name)
    if pos < 0:
        return assign_group_go(group, listAppend(axes, axis), listAppend(lengths, UNKNOWN_AXIS_LENGTH), j + 1, listAppend(positions, len(axes)))
    else:
        return assign_group_go(group, axes, lengths, j + 1, listAppend(positions, pos))


def assign_group(axes, lengths, group):
    """Assign elementary positions to one group's axes, appending new axes."""
    return assign_group_go(group, axes, lengths, 0, [])


def assign_side_go(comp, axes, lengths, i, positions) -> SideAssigned:
    if i >= len(comp):
        return SideAssigned(axes=axes, lengths=lengths, positions=positions)
    else:
        return assign_side_group(comp, axes, lengths, i, positions, unwrap_or(listGet(comp, i), []))


def assign_side_group(comp, axes, lengths, i, positions, group) -> SideAssigned:
    assigned = assign_group(axes, lengths, group)
    return assign_side_go(comp, assigned.axes, assigned.lengths, i + 1, listAppend(positions, assigned.positions))


def assign_side(comp, axes, lengths):
    """Assign positions for a whole composition; returns positions per group."""
    return assign_side_go(comp, axes, lengths, 0, [])


def validate_axes_go(axes_names, axes, i) -> Result[str, str]:
    if i >= len(axes_names):
        return Success(value="")
    else:
        name = unwrap_or(listGet(axes_names, i), "")
        if not check_axis_name(name):
            return Error(message=f"('Invalid name for an axis', '{name}')")
        elif find_named(axes, name) < 0:
            return Error(message=f"Axis {name} is not used in transform")
        else:
            return validate_axes_go(axes_names, axes, i + 1)


def mark_axes_go(axes_names, axes, lengths, i) -> list[int]:
    if i >= len(axes_names):
        return lengths
    else:
        pos = find_named(axes, unwrap_or(listGet(axes_names, i), ""))
        return mark_axes_go(axes_names, axes, set_at(lengths, pos, EXPECTED_AXIS_LENGTH), i + 1)


def apply_axes_lengths(axes_names, axes, lengths) -> Result[str, str]:
    """Validate caller-supplied axis lengths."""
    return validate_axes_go(axes_names, axes, 0)


def name2pos_of(axes_names, axes) -> list[NamePos]:
    """The supplied axes' name-to-elementary-position pairs."""
    return toList(map(range(len(axes_names)), lambda i: NamePos(name=unwrap_or(listGet(axes_names, i), ""), pos=find_named(axes, unwrap_or(listGet(axes_names, i), "")))))


def known_unknown_go(left_positions, axes, lengths, g, acc) -> Result[list[list[list[int]]], str]:
    if g >= len(left_positions):
        return Success(value=acc)
    else:
        return known_unknown_group(left_positions, axes, lengths, g, acc, unwrap_or(listGet(left_positions, g), []))


def known_unknown_group(left_positions, axes, lengths, g, acc, group) -> Result[list[list[list[int]]], str]:
    unknown = filterList(group, lambda p: unwrap_or(listGet(lengths, p), 0) == UNKNOWN_AXIS_LENGTH)
    if len(unknown) > 1:
        unknown_axes = toList(map(range(len(unknown)), lambda i: unwrap_or(listGet(axes, unwrap_or(listGet(unknown, i), 0)), named(""))))
        return Error(message=f"Could not infer sizes for {format_axes(unknown_axes)}")
    else:
        known = filterList(group, lambda p: unwrap_or(listGet(lengths, p), 0) != UNKNOWN_AXIS_LENGTH)
        return known_unknown_go(left_positions, axes, lengths, g + 1, listAppend(acc, [known, unknown]))


def known_unknown_checked(left_positions, axes, lengths) -> Result[list[list[list[int]]], str]:
    """Each input group's known/unknown positions, rejecting >1 unknown per group."""
    return known_unknown_go(left_positions, axes, lengths, 0, [])


def added_axis_go(right_flat, left_idents, axes, i, acc) -> list[PosElem]:
    if i >= len(right_flat):
        return acc
    else:
        return added_axis_step(right_flat, left_idents, axes, i, acc, unwrap_or(listGet(right_flat, i), 0))


def added_axis_step(right_flat, left_idents, axes, i, acc, p) -> list[PosElem]:
    if axis_in_idents(left_idents, unwrap_or(listGet(axes, p), named(""))):
        return added_axis_go(right_flat, left_idents, axes, i + 1, acc)
    else:
        return added_axis_go(right_flat, left_idents, axes, i + 1, listAppend(acc, PosElem(pos=i, elementary=p)))


def added_axis_entries(right_flat, left_idents, axes) -> list[PosElem]:
    """The added axes: output position to elementary position, in output order."""
    return added_axis_go(right_flat, left_idents, axes, 0, [])


def recipe_from(left, right, pattern, operation, axes_names, ndim):
    """Assemble the recipe from two parsed sides, after the agreement checks."""
    if left.has_ellipsis:
        n_other = len(left.composition) - 1
        if ndim < n_other:
            return Error(message=f"Wrong shape: expected >={n_other} dims. Received {ndim}-dim tensor.")
        else:
            ell_ndim = ndim - n_other
            ell_axes = toList(map(range(ell_ndim), lambda i: named(ELLIPSIS + toString(i))))
            left_comp = expand_left_comp(left.composition, ell_axes)
            rght_comp = expand_right_comp(right.composition, ell_axes)
            left_idents = listConcat(filterList(left.identifiers, lambda a: a.kind != "ellipsis"), ell_axes)
            right_idents = listConcat(filterList(right.identifiers, lambda a: a.kind != "ellipsis"), ell_axes) if right.has_ellipsis else right.identifiers
            return build_recipe(left_comp, rght_comp, left_idents, right_idents, axes_names)
    else:
        if ndim != len(left.composition):
            return Error(message=f"Wrong shape: expected {len(left.composition)} dims. Received {ndim}-dim tensor.")
        else:
            return build_recipe(composition_axes(left), composition_axes(right), left.identifiers, right.identifiers, axes_names)


def build_recipe(left_comp, rght_comp, left_idents, right_idents, axes_names):
    left_assigned = assign_side(left_comp, [], [])
    right_assigned = assign_side(rght_comp, left_assigned.axes, left_assigned.lengths)
    return build_recipe_bound(left_assigned, right_assigned, left_idents, right_idents, axes_names)


def build_recipe_bound(left_assigned, right_assigned, left_idents, right_idents, axes_names):
    axes = right_assigned.axes
    match apply_axes_lengths(axes_names, axes, right_assigned.lengths):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            return build_recipe_known(left_assigned, right_assigned, left_idents, right_idents, axes, mark_axes_go(axes_names, axes, right_assigned.lengths, 0), name2pos_of(axes_names, axes))


def build_recipe_known(left_assigned, right_assigned, left_idents, right_idents, axes, lengths, name2pos):
    match known_unknown_checked(left_assigned.positions, axes, lengths):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            left_flat = flatten(left_assigned.positions)
            right_flat = flatten(right_assigned.positions)
            reduced = filterList(left_flat, lambda p: not axis_in_idents(right_idents, unwrap_or(listGet(axes, p), named(""))))
            order_after = listConcat(filterList(right_flat, lambda p: axis_in_idents(left_idents, unwrap_or(listGet(axes, p), named("")))), reduced)
            permutation = toList(map(range(len(order_after)), lambda i: list_index(left_flat, unwrap_or(listGet(order_after, i), 0))))
            return Success(value=TransformRecipe(
                elementary_axes_lengths=lengths,
                axis_name2elementary_axis=listSortBy(name2pos, lambda a, b: 1 if a.name > b.name else (-1 if a.name < b.name else 0)),
                input_composition_known_unknown=value,
                axes_permutation=permutation,
                first_reduced_axis=len(order_after) - len(reduced),
                added_axes=added_axis_entries(right_flat, left_idents, axes),
                output_composite_axes=right_assigned.positions,
            ))


def prepare(pattern, operation, axes_names, ndim):
    """Parse `pattern` and build its transformation recipe."""
    match split(pattern, "->"):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            return prepare_parts(value, pattern, operation, axes_names, ndim)


def prepare_parts(parts, pattern, operation, axes_names, ndim):
    if len(parts) != 2:
        return Error(message=unpack_error(len(parts)))
    else:
        return prepare_left(parts, pattern, operation, axes_names, ndim)


def prepare_left(parts, pattern, operation, axes_names, ndim):
    rght_str = unwrap_or(listGet(parts, 1), "")
    match parse(unwrap_or(listGet(parts, 0), ""), False, False):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            return prepare_right(value, rght_str, pattern, operation, axes_names, ndim)


def prepare_right(left, rght_str, pattern, operation, axes_names, ndim):
    match parse(rght_str, False, False):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            return prepare_checked(left, value, pattern, operation, axes_names, ndim)


def prepare_checked(left, right, pattern, operation, axes_names, ndim):
    """The agreement checks, then `recipe_from`."""
    if not left.has_ellipsis and right.has_ellipsis:
        return Error(message=f"Ellipsis found in right side, but not left side of a pattern {pattern}")
    elif left.has_ellipsis and left.has_ellipsis_parenthesized:
        return Error(message=f"Ellipsis inside parenthesis in the left side is not allowed: {pattern}")
    elif operation == "rearrange":
        return check_rearrange(left, right, pattern, operation, axes_names, ndim)
    elif operation == "repeat":
        return check_repeat(left, right, pattern, operation, axes_names, ndim)
    elif is_reduction(operation):
        return check_reduce(left, right, pattern, operation, axes_names, ndim)
    else:
        return Error(message=f"Unknown reduction {operation}. Expect one of {REDUCTION_TEXT}.")


def check_rearrange(left, right, pattern, operation, axes_names, ndim):
    """Rearrange agreement: no non-unitary anonymous axes, and identical identifiers."""
    if left.has_non_unitary_anonymous_axes or right.has_non_unitary_anonymous_axes:
        return Error(message="Non-unitary anonymous axes are not supported in rearrange (exception is length 1)")
    else:
        return check_diff(left, right, pattern, operation, axes_names, ndim, symmetric_difference(left.identifiers, right.identifiers), "Identifiers only on one side of expression (should be on both)")


def check_repeat(left, right, pattern, operation, axes_names, ndim):
    """Repeat agreement: left identifiers all on the right, then sizes for new axes."""
    return check_repeat_diff(left, right, pattern, operation, axes_names, ndim, set_difference(left.identifiers, right.identifiers))


def check_repeat_diff(left, right, pattern, operation, axes_names, ndim, diff):
    if len(diff) > 0:
        return Error(message=f"Unexpected identifiers on the left side of repeat: {format_axes(diff)}")
    else:
        return check_repeat_size(left, right, pattern, operation, axes_names, ndim)


def check_repeat_size(left, right, pattern, operation, axes_names, ndim):
    without_size = filterList(right.identifiers, lambda ax: ax.kind == "named" and not axis_in_idents(left.identifiers, ax) and not listContains(axes_names, ax.name))
    if len(without_size) > 0:
        return Error(message=f"Specify sizes for new axes in repeat: {format_axes(without_size)}")
    else:
        return recipe_from(left, right, pattern, operation, axes_names, ndim)


def check_reduce(left, right, pattern, operation, axes_names, ndim):
    """Reduction agreement: right identifiers all on the left."""
    return check_diff(left, right, pattern, operation, axes_names, ndim, set_difference(right.identifiers, left.identifiers), f"Unexpected identifiers on the right side of reduce {operation}")


def check_diff(left, right, pattern, operation, axes_names, ndim, diff, message):
    """Reject a non-empty identifier difference, else build the recipe."""
    if len(diff) > 0:
        return Error(message=f"{message}: {format_axes(diff)}")
    else:
        return recipe_from(left, right, pattern, operation, axes_names, ndim)

