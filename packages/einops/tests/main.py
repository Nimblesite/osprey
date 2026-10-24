# osprey: flavor=python
"""Implements [PORT-SUITE]: parser, recipe and reconstruction assertions
against the unchanged CPython oracle, including every rejection message."""
from einops import Parsing, Recipe, Reconstruct
from einops_expected import Expected
from testing import Check


def at(xs, i, fallback):
    return unwrap_or(listGet(xs, i), fallback)


def ints(xs):
    return join(mapList(xs, lambda x: toString(x)), ",")


def groups(xs):
    return join(mapList(xs, ints), ";")


def axis_text(axis):
    return "anonymous:" + toString(axis.value) if axis.kind == "anonymous" else ("ellipsis" if axis.kind == "ellipsis" else "named:" + axis.name)


def ordered_identifiers(axes):
    named = listSortBy(filterList(axes, lambda a: a.kind == "named"), lambda a, b: 1 if a.name > b.name else (-1 if a.name < b.name else 0))
    anonymous = listSortBy(filterList(axes, lambda a: a.kind == "anonymous"), lambda a, b: a.value - b.value)
    return listConcat(listConcat(named, anonymous), filterList(axes, lambda a: a.kind == "ellipsis"))


def composite_text(group):
    return "ellipsis" if group.kind == "ellipsis" else "group:" + join(mapList(group.axes, axis_text), ",")


def flag(value):
    return "T" if value else "F"


def parsed_text(value):
    return join([value.expression, flag(value.has_ellipsis), flag(value.has_ellipsis_parenthesized), flag(value.has_non_unitary_anonymous_axes), join(mapList(ordered_identifiers(value.identifiers), axis_text), ","), join(mapList(value.composition, composite_text), ";")], "|")


def recipe_text(value):
    names = listSortBy(value.axis_name2elementary_axis, lambda a, b: 1 if a.name > b.name else (-1 if a.name < b.name else 0))
    named = join(mapList(names, lambda entry: entry.name + ":" + toString(entry.pos)), ";")
    known = join(mapList(value.input_composition_known_unknown, lambda pair: ints(at(pair, 0, [])) + "/" + ints(at(pair, 1, []))), ";")
    added = join(mapList(value.added_axes, lambda entry: toString(entry.pos) + ":" + toString(entry.elementary)), ";")
    return join([ints(value.elementary_axes_lengths), named, known, ints(value.axes_permutation), toString(value.first_reduced_axis), added, groups(value.output_composite_axes)], "|")


def cooked_text(value):
    added = join(mapList(value.added_axes, lambda entry: toString(entry.pos) + ":" + toString(entry.length)), ";")
    return join([ints(value.init_shapes), ints(value.axes_reordering), ints(value.reduced_axes), added, ints(value.final_shapes), toString(value.n_axes_after_adding_axes)], "|")


def expect_value(name, result, render, want):
    match result:
        case Success(value=value):
            return Check.equal(name, render(value), want)
        case Error(message=message):
            return Check.failed(name, message)


def expect_error(name, result, want):
    match result:
        case Success(value=value):
            return Check.failed(name, "invalid input was accepted")
        case Error(message=message):
            return Check.equal(name, message, want)


def parse_case(i):
    flags = at(Expected.PARSE_FLAGS, i, [False, False])
    result = Parsing.parse(at(Expected.PARSE_EXPRESSIONS, i, ""), at(flags, 0, False), at(flags, 1, False))
    return expect_value(f"parse.{i}", result, parsed_text, at(Expected.PARSE_TEXTS, i, ""))


def parse_error(i):
    result = Parsing.parse(at(Expected.PARSE_ERROR_EXPRESSIONS, i, ""), False, False)
    return expect_error(f"parse.error.{i}", result, at(Expected.PARSE_ERRORS, i, ""))


def recipe_case(i):
    result = Recipe.prepare(at(Expected.RECIPE_PATTERNS, i, ""), at(Expected.RECIPE_OPERATIONS, i, ""), at(Expected.RECIPE_AXES_NAMES, i, []), at(Expected.RECIPE_NDIMS, i, 0))
    return expect_value(f"recipe.{i}", result, recipe_text, at(Expected.RECIPE_TEXTS, i, ""))


def recipe_error(i):
    result = Recipe.prepare(at(Expected.RECIPE_ERROR_PATTERNS, i, ""), at(Expected.RECIPE_ERROR_OPERATIONS, i, ""), at(Expected.RECIPE_ERROR_AXES_NAMES, i, []), at(Expected.RECIPE_ERROR_NDIMS, i, 0))
    return expect_error(f"recipe.error.{i}", result, at(Expected.RECIPE_ERRORS, i, ""))


def reconstruct_result(pattern, operation, axes, ndim, shape, names, lengths):
    match Recipe.prepare(pattern, operation, axes, ndim):
        case Success(value=value):
            bound = toList(map(range(len(names)), lambda i: Reconstruct.NameLen(name=at(names, i, ""), length=at(lengths, i, 0))))
            return Reconstruct.reconstruct(value, shape, bound)
        case Error(message=message):
            return Error(message=message)


def reconstruct_case(i):
    result = reconstruct_result(at(Expected.RECONSTRUCT_PATTERNS, i, ""), at(Expected.RECONSTRUCT_OPERATIONS, i, ""), at(Expected.RECONSTRUCT_AXES_NAMES, i, []), at(Expected.RECONSTRUCT_NDIMS, i, 0), at(Expected.RECONSTRUCT_SHAPES, i, []), at(Expected.RECONSTRUCT_LENGTH_NAMES, i, []), at(Expected.RECONSTRUCT_LENGTH_VALUES, i, []))
    return expect_value(f"reconstruct.{i}", result, cooked_text, at(Expected.RECONSTRUCT_TEXTS, i, ""))


def reconstruct_error(i):
    result = reconstruct_result(at(Expected.RECONSTRUCT_ERROR_PATTERNS, i, ""), at(Expected.RECONSTRUCT_ERROR_OPERATIONS, i, ""), at(Expected.RECONSTRUCT_ERROR_AXES_NAMES, i, []), at(Expected.RECONSTRUCT_ERROR_NDIMS, i, 0), at(Expected.RECONSTRUCT_ERROR_SHAPES, i, []), at(Expected.RECONSTRUCT_ERROR_LENGTH_NAMES, i, []), at(Expected.RECONSTRUCT_ERROR_LENGTH_VALUES, i, []))
    return expect_error(f"reconstruct.error.{i}", result, at(Expected.RECONSTRUCT_ERRORS, i, ""))


def parser_contract():
    axis = Parsing.Axis(kind="named", name="a", value=0)
    group = Parsing.Composite(kind="group", axes=[axis])
    parsed = Parsing.ParsedExpression(expression="a", has_ellipsis=False, has_ellipsis_parenthesized=False, has_non_unitary_anonymous_axes=False, identifiers=[axis], composition=[group])
    return [Check.truth("named axis", structuralEquals(Parsing.named("a"), axis), "named axis differs"), Check.equal("axis repr", Parsing.axis_repr(axis), "'a'"), Check.equal("axis set", Parsing.format_axes([Parsing.named("b"), axis]), "{'a', 'b'}"), Check.equal("axis membership", Parsing.axis_in_idents([axis], Parsing.named("a")), True), Check.equal("absent axis", Parsing.axis_in_idents([axis], Parsing.named("b")), False), Check.truth("flat order", structuralEquals(Parsing.flat_axes_order(parsed), [axis]), "axis order differs"), Check.equal("uncomposed", Parsing.has_composed_axes(parsed), False), Check.equal("ellipsis", Parsing.ELLIPSIS, "…"), Check.equal("valid identifier", Parsing.check_axis_name("axis"), True), Check.truth("invalid identifier", structuralEquals(Parsing.check_axis_name_return_reason("1a", False), Parsing.AxisNameCheck(is_valid=False, reason="not a valid python identifier")), "identifier verdict differs"), Check.ints_equal("immutable set", Recipe.set_at([1, 2, 3], 1, 9), [1, 9, 3])]


def recipe_contract():
    expected = Recipe.TransformRecipe(elementary_axes_lengths=[-999999, -999999], axis_name2elementary_axis=[], input_composition_known_unknown=[[[], [0]], [[], [1]]], axes_permutation=[1, 0], first_reduced_axis=2, added_axes=[], output_composite_axes=[[1], [0]])
    return [expect_value("recipe type", Recipe.prepare("a b -> b a", "rearrange", [], 2), recipe_text, recipe_text(expected)), Check.equal("name position", Recipe.NamePos(name="h", pos=2).pos, 2), Check.equal("added position", Recipe.PosElem(pos=1, elementary=2).elementary, 2), Check.equal("added length", Reconstruct.PosLen(pos=1, length=4).length, 4), expect_value("cooked type", Reconstruct.reconstruct(expected, [3, 4], []), cooked_text, cooked_text(Reconstruct.CookedRecipe(init_shapes=[], axes_reordering=[1, 0], reduced_axes=[], added_axes=[], final_shapes=[], n_axes_after_adding_axes=2)))]


def cases():
    groups = [toList(map(range(len(Expected.PARSE_TEXTS)), parse_case)), toList(map(range(len(Expected.PARSE_ERRORS)), parse_error)), toList(map(range(len(Expected.RECIPE_TEXTS)), recipe_case)), toList(map(range(len(Expected.RECIPE_ERRORS)), recipe_error)), toList(map(range(len(Expected.RECONSTRUCT_TEXTS)), reconstruct_case)), toList(map(range(len(Expected.RECONSTRUCT_ERRORS)), reconstruct_error)), parser_contract(), recipe_contract()]
    return foldList(groups, [], listConcat)


def main():
    match Check.strictly(lambda: Check.report(cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
