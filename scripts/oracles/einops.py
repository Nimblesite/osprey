"""CPython oracle for @osprey/einops: parser, recipe and reconstruct verdicts.

Writes packages/einops/tests/expected.py, which the suite compares `Parsing`
and `Recipe` against ([PORT-TEST-ORACLE], spec 0043). Run with the oracle
virtualenv: `python scripts/oracles/einops.py`.

Every case is computed by the real einops (0.8.2). Values are recorded as
canonical `|`-separated strings (Osprey lists are homogeneous, so each field is
a flat list of strings), and error messages are compared exactly. A message
that formats a Python set of more than one identifier has its elements sorted
here, because CPython's set iteration order is hash-randomized and varies
between processes, while the port emits them sorted.
"""
import os
import sys

from expected_module import write

import einops
from einops import EinopsError
from einops.einops import _prepare_transformation_recipe, _reconstruct_from_shape_uncached
from einops.parsing import AnonymousAxis, ParsedExpression, _ellipsis


def axis_text(x):
    if x == _ellipsis:
        return "ellipsis"
    if isinstance(x, AnonymousAxis):
        return "anonymous:%d" % x.value
    return "named:%s" % x


def identifiers_sorted(idents):
    named = sorted((a for a in idents if isinstance(a, str) and a != _ellipsis), key=lambda a: a)
    anon = sorted((a for a in idents if isinstance(a, AnonymousAxis)), key=lambda a: a.value)
    triples = [axis_text(a) for a in named] + [axis_text(a) for a in anon]
    if _ellipsis in idents:
        triples.append(axis_text(_ellipsis))
    return triples


def composite_text(c):
    if c == _ellipsis:
        return "ellipsis"
    return "group:" + ",".join(axis_text(a) for a in c)


def parse_text(expression, p):
    return "|".join([
        expression,
        "T" if p.has_ellipsis else "F",
        "T" if (p.has_ellipsis_parenthesized is True) else "F",
        "T" if p.has_non_unitary_anonymous_axes else "F",
        ",".join(identifiers_sorted(p.identifiers)),
        ";".join(composite_text(c) for c in p.composition),
    ])


def il(xs):
    return ",".join(str(x) for x in xs)


def recipe_text(r):
    name2pos = ";".join("%s:%d" % (name, pos) for name, pos in sorted(r.axis_name2elementary_axis.items()))
    known_unknown = ";".join(il(sorted(known)) + "/" + il(sorted(unknown)) for known, unknown in r.input_composition_known_unknown)
    added = ";".join("%d:%d" % (pos, elem) for pos, elem in sorted(r.added_axes.items()))
    output = ";".join(il(g) for g in r.output_composite_axes)
    return "|".join([il(r.elementary_axes_lengths), name2pos, known_unknown, il(r.axes_permutation),
                     str(r.first_reduced_axis), added, output])


def cooked_text(c):
    init_shapes, axes_reordering, reduced_axes, added_axes, final_shapes, n_axes = c
    added = ";".join("%d:%d" % (pos, length) for pos, length in sorted(added_axes.items()))
    return "|".join([il(init_shapes if init_shapes is not None else []),
                     il(axes_reordering if axes_reordering is not None else []),
                     il(reduced_axes), added,
                     il(final_shapes if final_shapes is not None else []),
                     str(n_axes)])


def normalize_set(msg):
    if "{" not in msg or "}" not in msg:
        return msg
    start = msg.index("{")
    end = msg.rindex("}")
    items = [x.strip() for x in msg[start + 1:end].split(",")]
    items.sort()
    return msg[:start] + "{" + ", ".join(items) + "}" + msg[end + 1:]


def error_text(fn):
    try:
        fn()
        return "NO ERROR"
    except (EinopsError, ValueError) as e:
        return normalize_set(str(e))


PARSE_CASES = [
    ("a b", False, False),
    ("b c (h w)", False, False),
    ("1", False, False),
    ("a 1 b", False, False),
    ("2", False, False),
    ("10 a", False, False),
    ("a (b c) d", False, False),
    ("... a", False, False),
    ("a ...", False, False),
    ("...", False, False),
    ("(...) a", False, False),
    ("a ... (b c)", False, False),
    ("a_1 b", False, False),
    ("a2", False, False),
    ("axis", False, False),
    ("class", False, False),
    ("_", True, False),
    ("_ _", True, False),
    ("a a", False, True),
    ("a b", True, True),
]

PARSE_ERROR_CASES = [
    ("a.b", False, False),
    ("a...b...c", False, False),
    ("a....b", False, False),
    ("a a", False, False),
    ("a (b (c))", False, False),
    ("(a", False, False),
    ("a)", False, False),
    ("a b!", False, False),
    ("1a", False, False),
    ("_a", False, False),
    ("0", False, False),
    ("_", False, False),
    ("a_", False, False),
]

RECIPE_CASES = [
    ("a b -> b a", "rearrange", [], 2),
    ("a b -> a b", "rearrange", [], 2),
    ("b c (h w) -> b c h w", "rearrange", ["h"], 3),
    ("a (b c) -> (a b) c", "rearrange", ["b"], 2),
    ("a 1 b -> a b", "rearrange", [], 3),
    ("... a -> ... a", "rearrange", [], 3),
    ("a ... -> ... a", "rearrange", [], 2),
    ("a b c -> (a b) c", "rearrange", [], 3),
    ("1 a b -> a b", "rearrange", [], 3),
    ("a b -> a", "sum", [], 2),
    ("a b c -> a c", "mean", [], 3),
    ("a (b c) -> a", "max", ["b"], 2),
    ("a 2 b -> a b", "sum", [], 3),
    ("a b -> a", "prod", [], 2),
    ("... a -> a", "sum", [], 3),
    ("a ... -> ...", "mean", [], 2),
    ("a b c -> a c", "any", [], 3),
    ("a b c -> a c", "all", [], 3),
    ("a -> a b", "repeat", ["b"], 1),
    ("a b -> a b c", "repeat", ["c"], 2),
    ("a -> a 2", "repeat", [], 1),
    ("h w -> h (tile w)", "repeat", ["tile"], 2),
]

RECIPE_ERROR_CASES = [
    ("a -> ... a", "rearrange", [], 1),
    ("(...) a -> ... a", "rearrange", [], 3),
    ("2 a -> 2 a", "rearrange", [], 2),
    ("a b c -> a b", "rearrange", [], 3),
    ("a b -> a b c", "repeat", [], 2),
    ("a b c -> a b", "repeat", [], 3),
    ("a -> a b", "sum", [], 1),
    ("a b -> a", "foo", [], 2),
    ("a b -> a b", "rearrange", [], 3),
    ("... a -> ... a", "rearrange", [], 0),
    ("(a b) -> a", "sum", [], 1),
    ("a -> a b", "repeat", ["_x"], 1),
    ("a b -> a b", "rearrange", ["c"], 2),
    ("a -> a 2", "sum", [], 1),
    ("a b", "rearrange", [], 2),
]

RECONSTRUCT_CASES = [
    ("a b -> b a", "rearrange", [], 2, [3, 4], []),
    ("b c (h w) -> b c h w", "rearrange", ["h"], 3, [2, 3, 12], [["h", 4]]),
    ("a (b c) -> (a b) c", "rearrange", ["b"], 2, [6, 12], [["b", 3]]),
    ("a 1 b -> a b", "rearrange", [], 3, [3, 1, 5], []),
    ("... a -> ... a", "rearrange", [], 3, [2, 3, 5], []),
    ("a ... -> ... a", "rearrange", [], 2, [4, 7], []),
    ("a b -> a", "sum", [], 2, [3, 4], []),
    ("a (b c) -> a", "max", ["b"], 2, [3, 4], [["b", 2]]),
    ("a 2 b -> a b", "sum", [], 3, [3, 2, 5], []),
    ("... a -> a", "sum", [], 3, [2, 3, 5], []),
    ("a ... -> ...", "mean", [], 2, [4, 7], []),
    ("a -> a b", "repeat", ["b"], 1, [3], [["b", 4]]),
    ("a -> a 2", "repeat", [], 1, [3], []),
    ("h w -> h (tile w)", "repeat", ["tile"], 2, [4, 5], [["tile", 3]]),
]

RECONSTRUCT_ERROR_CASES = [
    ("a 1 b -> a b", "rearrange", [], 3, [3, 2, 5], []),
    ("b c (h w) -> b c h w", "rearrange", ["h"], 3, [2, 3, 10], [["h", 4]]),
    ("a b -> a", "sum", ["a"], 2, [3, 4], [["a", 5]]),
]


def recipe_case_columns(cases):
    return ([c[0] for c in cases], [c[1] for c in cases], [c[2] for c in cases], [c[3] for c in cases])


def reconstruct_columns(cases):
    return ([c[0] for c in cases], [c[1] for c in cases], [c[2] for c in cases], [c[3] for c in cases],
            [c[4] for c in cases], [[name for name, _ in c[5]] for c in cases], [[length for _, length in c[5]] for c in cases])


def build():
    constants = {}
    constants["PARSE_EXPRESSIONS"] = [c[0] for c in PARSE_CASES]
    constants["PARSE_FLAGS"] = [[u, d] for _, u, d in PARSE_CASES]
    constants["PARSE_TEXTS"] = [parse_text(e, ParsedExpression(e, allow_underscore=u, allow_duplicates=d)) for e, u, d in PARSE_CASES]
    constants["PARSE_ERROR_EXPRESSIONS"] = [c[0] for c in PARSE_ERROR_CASES]
    constants["PARSE_ERRORS"] = [error_text(lambda c=c: ParsedExpression(c[0], allow_underscore=c[1], allow_duplicates=c[2])) for c in PARSE_ERROR_CASES]

    rp, ro, ra, rn = recipe_case_columns(RECIPE_CASES)
    constants["RECIPE_PATTERNS"] = rp
    constants["RECIPE_OPERATIONS"] = ro
    constants["RECIPE_AXES_NAMES"] = ra
    constants["RECIPE_NDIMS"] = rn
    constants["RECIPE_TEXTS"] = [recipe_text(_prepare_transformation_recipe(p, o, tuple(a), n)) for p, o, a, n in RECIPE_CASES]

    ep, eo, ea, en = recipe_case_columns(RECIPE_ERROR_CASES)
    constants["RECIPE_ERROR_PATTERNS"] = ep
    constants["RECIPE_ERROR_OPERATIONS"] = eo
    constants["RECIPE_ERROR_AXES_NAMES"] = ea
    constants["RECIPE_ERROR_NDIMS"] = en
    constants["RECIPE_ERRORS"] = [error_text(lambda c=c: _prepare_transformation_recipe(c[0], c[1], tuple(c[2]), c[3])) for c in RECIPE_ERROR_CASES]

    cp, co, ca, cn, cs, cln, clv = reconstruct_columns(RECONSTRUCT_CASES)
    constants["RECONSTRUCT_PATTERNS"] = cp
    constants["RECONSTRUCT_OPERATIONS"] = co
    constants["RECONSTRUCT_AXES_NAMES"] = ca
    constants["RECONSTRUCT_NDIMS"] = cn
    constants["RECONSTRUCT_SHAPES"] = cs
    constants["RECONSTRUCT_LENGTH_NAMES"] = cln
    constants["RECONSTRUCT_LENGTH_VALUES"] = clv
    constants["RECONSTRUCT_TEXTS"] = [cooked_text(_reconstruct_from_shape_uncached(
        _prepare_transformation_recipe(p, o, tuple(a), n), list(s), [(name, length) for name, length in al]))
        for p, o, a, n, s, al in RECONSTRUCT_CASES]

    xp, xo, xa, xn, xs, xln, xlv = reconstruct_columns(RECONSTRUCT_ERROR_CASES)
    constants["RECONSTRUCT_ERROR_PATTERNS"] = xp
    constants["RECONSTRUCT_ERROR_OPERATIONS"] = xo
    constants["RECONSTRUCT_ERROR_AXES_NAMES"] = xa
    constants["RECONSTRUCT_ERROR_NDIMS"] = xn
    constants["RECONSTRUCT_ERROR_SHAPES"] = xs
    constants["RECONSTRUCT_ERROR_LENGTH_NAMES"] = xln
    constants["RECONSTRUCT_ERROR_LENGTH_VALUES"] = xlv
    constants["RECONSTRUCT_ERRORS"] = [error_text(lambda c=c: _reconstruct_from_shape_uncached(
        _prepare_transformation_recipe(c[0], c[1], tuple(c[2]), c[3]), list(c[4]),
        [(name, length) for name, length in c[5]])) for c in RECONSTRUCT_ERROR_CASES]
    return constants


if __name__ == "__main__":
    write("einops", "ParsedExpression fields, transformation recipes, cooked reshapes and every EinopsError message text.", {"einops": einops.__version__}, build())
