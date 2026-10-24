# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=einops
# osprey: module=Parsing
"""The einops pattern language: one side of an expression such as `b c (h w)`.

Parses named axes, anonymous axes spelled `2` (or any decimal length), the
`(h w)` one-level composition, and the `...` ellipsis, applying the same
identifier rules and error text as `einops.parsing.ParsedExpression`
(einops 0.8.2). Implements [PORT-SCOPE]; every error is a `Result` whose
message equals `str(EinopsError)` from the original.
"""

__all__ = [
    "ELLIPSIS", "Axis", "Composite", "ParsedExpression", "AxisNameCheck",
    "parse", "check_axis_name_return_reason", "check_axis_name",
    "flat_axes_order", "has_composed_axes", "axis_repr", "format_axes",
    "named", "axis_in_idents",
]

ELLIPSIS = "…"


class Axis:
    """One elementary axis. `kind` is `"named"`, `"anonymous"` or `"ellipsis"`;
    `name` holds a named axis's identifier and `value` an anonymous length."""
    kind: str
    name: str
    value: int


class Composite:
    """One entry of a parsed composition: `kind` `"group"` holds a list of axes,
    `kind` `"ellipsis"` marks the single ellipsis slot."""
    kind: str
    axes: list[Axis]


class ParsedExpression:
    """One parsed side of an einops pattern."""
    expression: str
    has_ellipsis: bool
    has_ellipsis_parenthesized: bool
    has_non_unitary_anonymous_axes: bool
    identifiers: list[Axis]
    composition: list[Composite]


class AxisNameCheck:
    """The verdict of `check_axis_name_return_reason`."""
    is_valid: bool
    reason: str


def named(name):
    """A named axis."""
    return Axis(kind="named", name=name, value=0)


def anonymous(value):
    return Axis(kind="anonymous", name="", value=value)


def ellipsis_axis():
    return Axis(kind="ellipsis", name=ELLIPSIS, value=0)


def axis_repr(ax):
    """An axis as Python's `repr` prints it inside a set message: `'b'` for a
    named axis, `2-axis` for an anonymous one, `'…'` for the ellipsis."""
    if ax.kind == "named":
        return "'" + ax.name + "'"
    elif ax.kind == "anonymous":
        return toString(ax.value) + "-axis"
    else:
        return "'" + ELLIPSIS + "'"


def format_axes(axes):
    """`{'a', 'b'}`: a set of axes, sorted by `axis_repr`, as an error names it."""
    sorted_axes = listSortBy(axes, lambda a, b: 1 if axis_repr(a) > axis_repr(b) else (-1 if axis_repr(a) < axis_repr(b) else 0))
    return "{" + join(toList(map(range(len(sorted_axes)), lambda i: axis_repr(unwrap_or(listGet(sorted_axes, i), named(""))))), ", ") + "}"


def cp_digit(cp):
    return cp >= 48 and cp <= 57


def cp_letter(cp):
    return (cp >= 65 and cp <= 90) or (cp >= 97 and cp <= 122)


def is_identifier(name):
    """Python's `str.isidentifier` restricted to ASCII letters, digits and `_`."""
    match byteAt(name, 0):
        case Success(value=value):
            return (cp_letter(value) or value == 95) and all_chars(name, lambda cp: cp_letter(cp) or cp_digit(cp) or cp == 95)
        case Error():
            return False


def all_chars_go(s, pred, i) -> bool:
    if i >= length(s):
        return True
    else:
        return pred(unwrap_or(byteAt(s, i), 0)) and all_chars_go(s, pred, i + 1)


def all_chars(s, pred):
    """True when every byte of `s` satisfies `pred`."""
    return all_chars_go(s, pred, 0)


def is_decimal(x):
    """`str.isdecimal` for ASCII: non-empty and every character is a digit."""
    if length(x) == 0:
        return False
    else:
        return all_chars(x, cp_digit)


def check_axis_name_return_reason(name, allow_underscore):
    """The original's identifier rule, with its exact reasons."""
    if not is_identifier(name):
        return AxisNameCheck(is_valid=False, reason="not a valid python identifier")
    elif startsWith(name, "_") or endsWith(name, "_"):
        if name == "_" and allow_underscore:
            return AxisNameCheck(is_valid=True, reason="")
        else:
            return AxisNameCheck(is_valid=False, reason="axis name should should not start or end with underscore")
    else:
        return AxisNameCheck(is_valid=True, reason="")


def check_axis_name(name):
    """Whether `name` is an acceptable axis name."""
    return check_axis_name_return_reason(name, False).is_valid


def axis_in_idents(idents, ax):
    """Membership in an identifier set: named axes by name, the ellipsis by kind,
    and anonymous axes never (each occurrence is a distinct axis)."""
    if ax.kind == "anonymous":
        return False
    elif ax.kind == "ellipsis":
        return len(filterList(idents, lambda i: i.kind == "ellipsis")) > 0
    else:
        return len(filterList(idents, lambda i: i.kind == "named" and i.name == ax.name)) > 0


def token_axis(x):
    """An identifier token's Axis for the duplicate check: numbers never collide,
    because the original stores anonymous axes as distinct objects."""
    if x == ELLIPSIS:
        return ellipsis_axis()
    elif is_decimal(x):
        return anonymous(unwrap_or(parseInt(x), 0))
    else:
        return named(x)


class Scan:
    current: str
    has_current: bool
    in_bracket: bool
    bracket: list[Axis]
    composition: list[Composite]
    identifiers: list[Axis]
    has_non_unitary_anonymous: bool
    ellipsis_parenthesized: bool


def initial_scan():
    return Scan(current="", has_current=False, in_bracket=False, bracket=[], composition=[], identifiers=[], has_non_unitary_anonymous=False, ellipsis_parenthesized=False)


def add_ellipsis(state):
    idents = listAppend(state.identifiers, ellipsis_axis()) if not axis_in_idents(state.identifiers, ellipsis_axis()) else state.identifiers
    if not state.in_bracket:
        return Success(value=update(state, has_current=False, current="", identifiers=idents, composition=listAppend(state.composition, Composite(kind="ellipsis", axes=[])), ellipsis_parenthesized=False))
    else:
        return Success(value=update(state, has_current=False, current="", identifiers=idents, bracket=listAppend(state.bracket, ellipsis_axis()), ellipsis_parenthesized=True))


def add_number(state, x):
    value = unwrap_or(parseInt(x), 0)
    if value == 1:
        if not state.in_bracket:
            return Success(value=update(state, has_current=False, current="", composition=listAppend(state.composition, Composite(kind="group", axes=[]))))
        else:
            return Success(value=update(state, has_current=False, current=""))
    elif value < 1:
        return Error(message=f"Anonymous axis should have positive length, not {value}")
    else:
        ax = anonymous(value)
        idents = listAppend(state.identifiers, ax)
        if not state.in_bracket:
            return Success(value=update(state, has_current=False, current="", identifiers=idents, has_non_unitary_anonymous=True, composition=listAppend(state.composition, Composite(kind="group", axes=[ax]))))
        else:
            return Success(value=update(state, has_current=False, current="", identifiers=idents, has_non_unitary_anonymous=True, bracket=listAppend(state.bracket, ax)))


def add_named(state, x, allow_underscore):
    check = check_axis_name_return_reason(x, allow_underscore)
    if not check.is_valid:
        return Error(message=f"Invalid axis identifier: {x}\n{check.reason}")
    else:
        ax = named(x)
        idents = listAppend(state.identifiers, ax) if not axis_in_idents(state.identifiers, ax) else state.identifiers
        if not state.in_bracket:
            return Success(value=update(state, has_current=False, current="", identifiers=idents, composition=listAppend(state.composition, Composite(kind="group", axes=[ax]))))
        else:
            return Success(value=update(state, has_current=False, current="", identifiers=idents, bracket=listAppend(state.bracket, ax)))


def add_axis_body(state, x, allow_underscore):
    if x == ELLIPSIS:
        return add_ellipsis(state)
    elif is_decimal(x):
        return add_number(state, x)
    else:
        return add_named(state, x, allow_underscore)


def add_axis_name(state, x, allow_underscore, allow_duplicates):
    """The original `add_axis_name`: fold one identifier token into the scan."""
    if axis_in_idents(state.identifiers, token_axis(x)) and not (allow_underscore and x == "_") and not allow_duplicates:
        return Error(message=f'Indexing expression contains duplicate dimension "{x}"')
    else:
        return add_axis_body(state, x, allow_underscore)


def is_scan_char(c):
    """The characters the scan accumulates into an identifier."""
    return c == "_" or c == ELLIPSIS or (c >= "0" and c <= "9") or (c >= "a" and c <= "z") or (c >= "A" and c <= "Z")


def flush(state, allow_underscore, allow_duplicates):
    if state.has_current:
        return add_axis_name(state, state.current, allow_underscore, allow_duplicates)
    else:
        return Success(value=state)


def step_delimiter(state, c, allow_underscore, allow_duplicates):
    match flush(state, allow_underscore, allow_duplicates):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            if c == " ":
                return Success(value=value)
            elif c == "(":
                if value.in_bracket:
                    return Error(message="Axis composition is one-level (brackets inside brackets not allowed)")
                else:
                    return Success(value=update(value, in_bracket=True, bracket=[]))
            else:
                if not value.in_bracket:
                    return Error(message="Brackets are not balanced")
                else:
                    group = Composite(kind="group", axes=value.bracket)
                    return Success(value=update(value, in_bracket=False, bracket=[], composition=listAppend(value.composition, group)))


def step_char(state, c, allow_underscore, allow_duplicates):
    """Advance the scan over one logical character."""
    if c == "(" or c == ")" or c == " ":
        return step_delimiter(state, c, allow_underscore, allow_duplicates)
    elif is_scan_char(c):
        if state.has_current:
            return Success(value=update(state, current=state.current + c))
        else:
            return Success(value=update(state, current=c, has_current=True))
    else:
        return Error(message=f"Unknown character '{c}'")


def scan_width(s, i, cp, state, allow_underscore, allow_duplicates) -> Result[Scan, str]:
    match codePointWidth(cp):
        case Success(value=value):
            return scan_step(s, i + value, cp, state, allow_underscore, allow_duplicates)
        case Error(message=message):
            return Error(message=message)


def scan_step(s, next_i, cp, state, allow_underscore, allow_duplicates) -> Result[Scan, str]:
    c = unwrap_or(fromCodePoint(cp), "?")
    match step_char(state, c, allow_underscore, allow_duplicates):
        case Success(value=value):
            return scan_chars(s, next_i, value, allow_underscore, allow_duplicates)
        case Error(message=message):
            return Error(message=message)


def scan_char_at(s, i, state, allow_underscore, allow_duplicates) -> Result[Scan, str]:
    match codePointAt(s, i):
        case Success(value=value):
            return scan_width(s, i, value, state, allow_underscore, allow_duplicates)
        case Error(message=message):
            return Error(message=message)


def scan_chars(s, i, state, allow_underscore, allow_duplicates) -> Result[Scan, str]:
    """Drive `step_char` over `s`'s codepoints."""
    if i >= length(s):
        return Success(value=state)
    else:
        return scan_char_at(s, i, state, allow_underscore, allow_duplicates)


def count_substring_go(sub, rest, n) -> int:
    if isEmpty(rest):
        return n
    elif startsWith(rest, sub):
        return count_substring_go(sub, drop(rest, length(sub)), n + 1)
    else:
        return count_substring_go(sub, drop(rest, 1), n)


def count_substring(s, sub):
    """Python's `str.count` for `sub`, non-overlapping."""
    return count_substring_go(sub, s, 0)


def count_char_go(c, rest, n) -> int:
    if isEmpty(rest):
        return n
    elif startsWith(rest, c):
        return count_char_go(c, drop(rest, 1), n + 1)
    else:
        return count_char_go(c, drop(rest, 1), n)


def count_char(s, c):
    return count_char_go(c, s, 0)


def sort_identifiers(idents):
    """The canonical order: named axes by name, anonymous by value, ellipsis last."""
    named_axes = listSortBy(filterList(idents, lambda a: a.kind == "named"), lambda a, b: 1 if a.name > b.name else (-1 if a.name < b.name else 0))
    anonymous_axes = listSortBy(filterList(idents, lambda a: a.kind == "anonymous"), lambda a, b: 1 if a.value > b.value else (-1 if a.value < b.value else 0))
    ellipsis_axes = filterList(idents, lambda a: a.kind == "ellipsis")
    return listConcat(named_axes, listConcat(anonymous_axes, ellipsis_axes))


def finish_parse(original, has_ellipsis, allow_underscore, allow_duplicates, done):
    match flush(done, allow_underscore, allow_duplicates):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            return Success(value=ParsedExpression(
                expression=original,
                has_ellipsis=has_ellipsis,
                has_ellipsis_parenthesized=value.ellipsis_parenthesized,
                has_non_unitary_anonymous_axes=value.has_non_unitary_anonymous,
                identifiers=sort_identifiers(value.identifiers),
                composition=value.composition,
            ))


def parse_replaced(original, replaced, has_ellipsis, allow_underscore, allow_duplicates):
    """Scan an expression whose `...` has already become the ellipsis marker."""
    match scan_chars(replaced, 0, initial_scan(), allow_underscore, allow_duplicates):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            if value.in_bracket:
                return Error(message=f'Imbalanced parentheses in expression: "{replaced}"')
            else:
                return finish_parse(original, has_ellipsis, allow_underscore, allow_duplicates, value)


def parse(expression, allow_underscore, allow_duplicates):
    """Parse one side of an einops pattern into a `ParsedExpression`."""
    if contains(expression, "."):
        if not contains(expression, "..."):
            return Error(message="Expression may contain dots only inside ellipsis (...)")
        elif count_substring(expression, "...") != 1 or count_char(expression, ".") != 3:
            return Error(message="Expression may contain dots only inside ellipsis (...); only one ellipsis for tensor ")
        else:
            replaced = unwrap_or(replace(expression, "...", ELLIPSIS), expression)
            return parse_replaced(expression, replaced, True, allow_underscore, allow_duplicates)
    else:
        return parse_replaced(expression, expression, contains(expression, ELLIPSIS), allow_underscore, allow_duplicates)


def flat_axes_order(expr):
    """Every axis of the composition in order, ignoring the ellipsis."""
    return foldList(expr.composition, [], lambda acc, c: listConcat(acc, c.axes))


def has_composed_axes(expr):
    """Whether any group composes more than one axis."""
    return len(filterList(expr.composition, lambda c: c.kind == "group" and len(c.axes) > 1)) > 0
