# osprey: flavor=python
# [PATTERN-BINDING-SCOPE] [PATTERN-OR] All four matching suites share one golden.

def classify(n):
    match n:
        case 0:
            return "zero"
        case 1:
            return "one"
        case _:
            return "many"

def literal_case():
    return checkAll("literal and wildcard arms", [
        answersHold(classify, [0, 1, 2, 9, -1, 100], ["zero", "one", "many", "many", "many", "many"]),
        classify(0) != classify(1)
    ])

def grade(n):
    match n >= 50:
        case True:
            return "pass"
        case False:
            return "fail"

def parity(n):
    match n % 2 == 0:
        case True:
            return "even"
        case False:
            return "odd"

# #433: a name every branch of an `if` binds is bound after it, as in CPython.
def size_or_zero(items):
    if len(items) > 0:
        result = wrapAdd(len(items), 100)
    else:
        result = 0
    return result

def band(n):
    if n < 0:
        name = "neg"
    elif n == 0:
        name = "zero"
    elif n < 10:
        doubled = wrapMul(n, 2)
        name = f"small{doubled}"
    else:
        name = "big"
    return f"{name}:{n}"

def quadrant(x, y):
    if x >= 0:
        if y >= 0:
            q = 1
        else:
            q = 4
    else:
        q = 2 if y >= 0 else 3
    scaled = wrapMul(q, 10)
    return scaled

def spread(a, b):
    if a <= b:
        low = a
        high = b
    else:
        low = b
        high = a
    return f"{low}..{high}"

def boolean_case():
    return checkAll("grade boundaries and parity", [
        f"g80={grade(80)} g20={grade(20)} e4={parity(4)} e7={parity(7)}" == "g80=pass g20=fail e4=even e7=odd",
        grade(50) == "pass", grade(49) == "fail", parity(0) == "even", parity(9) == "odd",
        size_or_zero([5, 6]) == 102, size_or_zero([]) == 0,
        answersHold(band, [-3, 0, 4, 9, 10, 99], ["neg:-3", "zero:0", "small8:4", "small18:9", "big:10", "big:99"]),
        quadrant(1, 1) == 10, quadrant(-1, 1) == 20, quadrant(-1, -1) == 30, quadrant(1, -1) == 40,
        quadrant(0, 0) == 10, spread(2, 7) == "2..7", spread(7, 2) == "2..7", spread(4, 4) == "4..4"
    ])

def label(n):
    match n:
        case 0:
            return "none"
        case 1:
            return "single"
        case 2:
            return "pair"
        case _:
            return "many"

def integer_case():
    return checkAll("integer arms and adjacent wildcards", [
        f"a={label(0)} b={label(1)} c={label(2)} d={label(7)}" == "a=none b=single c=pair d=many",
        answersHold(label, [0, 1, 2, 3, 7, -1, 100], ["none", "single", "pair", "many", "many", "many", "many"])
    ])

test("literal matching retains every printed arm", literal_case)
test("boolean matching covers thresholds and parity", boolean_case)
test("integer matching covers literals and wildcard", integer_case)

@union
class Choice:
    class Pick:
        value: int
    class Other:
        pass

def catch_reader(input):
    match input:
        case value:
            return lambda: value

def binding_case():
    value = 100
    reader = catch_reader(2)
    return checkAll("catch-all scope", [reader() == 2, value == 100])

def result_scope(input, value, message):
    match input:
        case Success(value=value):
            return f"{value}:{message}"
        case Error(message=message):
            return f"{value}:{message}"

def list_scope(input, head):
    match input:
        case []:
            return head
        case [head, *tail]:
            return wrapAdd(head, len(tail))

def chosen_reader(input, value):
    match input:
        case Pick(value=value):
            return lambda: value
        case Other():
            return lambda: value

def sibling_case():
    value = 100
    message = "outer"
    return checkAll("sibling arms", [
        result_scope(Success(value=2), value, message) == "2:outer",
        result_scope(Error(message="bad"), value, message) == "100:bad",
        list_scope([2, 3], value) == 3, list_scope([], value) == 100,
        chosen_reader(Pick(value=2), value)() == 2, chosen_reader(Other(), value)() == 100,
        value == 100, message == "outer"
    ])

def captured_case():
    reader = chosen_reader(Pick(value=2), 100)
    fallback = chosen_reader(Other(), 100)
    return checkAll("escaping arm closures", [reader() == 2, fallback() == 100, reader() == 2])

def nested_choice(outer, inner, value):
    match outer:
        case Pick(value=value):
            selected = chosen_reader(inner, value)
            return f"{selected()}:{value}"
        case Other():
            return f"none:{value}"

def nested_case():
    return checkAll("nested patterns", [
        nested_choice(Pick(value=2), Pick(value=3), 100) == "3:2",
        nested_choice(Pick(value=2), Other(), 100) == "2:2",
        nested_choice(Other(), Pick(value=3), 100) == "none:100"
    ])

@effect
class PatternCounter:
    def verify() -> bool: ...

@handler(PatternCounter)
class count_bindings:
    def verify():
        value = 100
        read_outer = lambda: value
        selected = chosen_reader(Pick(value=2), value)
        before = read_outer()
        value = 101
        return before == 100 and read_outer() == 101 and value == 101 and selected() == 2

def mutable_case():
    with count_bindings:
        return checkAll("mutable and captured bindings", [perform(PatternCounter.verify())])

test("catch-all bindings remain inside their arm", binding_case)
test("pattern arms retain their own lexical bindings", sibling_case)
test("escaping closures retain pattern bindings", captured_case)
test("nested matches restore enclosing pattern bindings", nested_case)
test("pattern bindings shadow captured mutable cells", mutable_case)

class IntOperation:
    def run(n: int) -> int: ...

def record_call(input):
    match input:
        case IntOperation(run=apply):
            return apply(2)

def unwrap_call(input):
    match input:
        case Success(value=apply):
            return apply(2)
        case Error(message=message):
            return len(message)

def list_call(callbacks):
    match callbacks:
        case []:
            return 0
        case [apply, *rest]:
            return wrapAdd(apply(2), len(rest))

def call_binding_case():
    run = lambda n: wrapAdd(n, 100)
    selected = record_call(IntOperation(run=lambda n: wrapAdd(n, 1)))
    return checkAll("callable record fields", [selected == 3, run(2) == 102])

def call_payload_case():
    outer = lambda n: wrapAdd(n, 100)
    return checkAll("callable payloads", [
        unwrap_call(Success(value=lambda n: wrapAdd(n, 1))) == 3,
        unwrap_call(Error(message="oops")) == 4,
        list_call([lambda n: wrapAdd(n, 2)]) == 4,
        list_call([lambda n: n, outer]) == 3, outer(2) == 102
    ])

def direct_call():
    match (lambda x: wrapAdd(x, 3)):
        case call:
            return call(2)

def direct_call_case():
    return checkAll("direct callable scrutinees", [
        direct_call() == 5,
        list_call([lambda x: wrapAdd(x, 2)]) == 4,
        unwrap_call(Success(value=lambda n: wrapAdd(n, 1))) == 3
    ])

def joined(flag):
    match flag:
        case True:
            return Success(value=lambda n: n * 2.0)
        case False:
            return Success(value=lambda n: n * 3.0)

def apply_joined(flag):
    match joined(flag):
        case Success(value=f):
            return f(1.5)
        case Error():
            return -1.0

def joined_case():
    return checkAll("float callback payloads", [apply_joined(True) == 3.0, apply_joined(False) == 4.5])

test("callable pattern bindings retain their own signatures", call_binding_case)
test("callable payload patterns preserve function values", call_payload_case)
test("direct callable scrutinees preserve function types", direct_call_case)
test("joined callable Results retain their payload signatures", joined_case)

@union
class Glyph:
    class Space:
        pass
    class Tab:
        pass
    class Newline:
        pass
    class Letter:
        c: str
    class Digit:
        d: int
    class Word:
        w: str

@union
class PointFamily:
    class Pt:
        x: int
        y: int

@union
class TripleFamily:
    class Tri:
        a: int
        b: int
        c: int

@union
class CellFamily:
    class Cell:
        g: Glyph
        n: int

@union
class Box:
    class Wrap:
        items: list[int]
    class Boxed:
        items: list[int]
    class Nest:
        items: list[list[int]]
    class Hollow:
        pass

class Sized:
    kind: str
    n: int

def or_glyph_kind(g):
    match g:
        case Space() | Tab() | Newline():
            return "blank"
        case Digit(0 | 1):
            return "bit"
        case Digit(d):
            return f"digit {d}"
        case Letter(s) | Word(s):
            return f"text {s}"

def or_small_int(n):
    match n:
        case 0 | 1 | 2:
            return "small"
        case _:
            return "big"

def or_yes(s):
    match s:
        case "y" | "yes" | "Y":
            return True
        case _:
            return False

def or_either(b):
    match b:
        case True | False:
            return "bool"

def or_unitish(x):
    match x:
        case 0.0 | 1.0:
            return "unit"
        case _:
            return "other"

def or_literal_case():
    return checkAll("or-pattern variant and literal alternatives", [
        or_glyph_kind(Space()) == "blank", or_glyph_kind(Tab()) == "blank", or_glyph_kind(Newline()) == "blank",
        or_glyph_kind(Digit(d=0)) == "bit", or_glyph_kind(Digit(d=1)) == "bit",
        or_glyph_kind(Digit(d=7)) == "digit 7",
        or_glyph_kind(Letter(c="a")) == "text a", or_glyph_kind(Word(w="hi")) == "text hi",
        or_small_int(0) == "small", or_small_int(1) == "small", or_small_int(2) == "small",
        or_small_int(3) == "big", or_small_int(-1) == "big",
        or_yes("y"), or_yes("yes"), or_yes("Y"), or_yes("n") == False, or_yes("") == False,
        or_either(True) == "bool", or_either(False) == "bool",
        or_unitish(0.0) == "unit", or_unitish(1.0) == "unit", or_unitish(0.5) == "other",
    ])

test("or-patterns join variant and literal alternatives", or_literal_case)

def or_result_text(r):
    match r:
        case Success(s) | Error(s):
            return f"got {s}"

def or_on_axis(p):
    match p:
        case Pt(0, _) | Pt(_, 0):
            return "axis"
        case _:
            return "plane"

def or_short_list(xs):
    match xs:
        case [] | [_]:
            return "short"
        case [a, b, *rest]:
            return f"long {wrapAdd(wrapAdd(a, b), len(rest))}"

def or_axis_pair(x, y):
    match Pt(x=x, y=y):
        case Pt(0, n) | Pt(n, 0):
            return n
        case _:
            return -1

def or_tail_size(xs):
    match xs:
        case [0, *rest] | [1, *rest]:
            return len(rest)
        case _:
            return -1

def or_tail_len(b):
    match b:
        case Wrap([_, *xs]) | Boxed([_, _, *xs]) | Nest([xs]):
            return len(xs)
        case _:
            return -1

def or_row_size(r):
    match r:
        case Sized("s", n) | Sized("m", n):
            return n
        case _:
            return 0

def or_open_band(r):
    match r:
        case Sized("old", _):
            return 1
        case Sized("s" | "m", 2 | 3):
            return 2
        case _:
            return 0

def or_deep_blank(r):
    match r:
        case Success(Cell(Space() | Tab(), 0 | 1)) | Error("blank"):
            return "deep blank"
        case Success(Cell(Letter("x" | "y"), _)):
            return "deep letter"
        case _:
            return "deep other"

def or_structure_case():
    return checkAll("or-pattern Result, payload, list, row and nested alternatives", [
        or_result_text(Success(value="a")) == "got a", or_result_text(Error(message="b")) == "got b",
        or_on_axis(Pt(x=0, y=5)) == "axis", or_on_axis(Pt(x=5, y=0)) == "axis", or_on_axis(Pt(x=0, y=0)) == "axis",
        or_on_axis(Pt(x=2, y=3)) == "plane",
        or_axis_pair(0, 8) == 8, or_axis_pair(9, 0) == 9, or_axis_pair(2, 3) == -1,
        or_tail_size([0]) == 0, or_tail_size([1, 8, 9]) == 2, or_tail_size([2, 9]) == -1, or_tail_size([]) == -1,
        or_short_list([]) == "short", or_short_list([9]) == "short", or_short_list([1, 2, 3, 4]) == "long 5",
        or_tail_len(Wrap(items=[1, 2, 3])) == 2, or_tail_len(Boxed(items=[1, 2, 3])) == 1,
        or_tail_len(Nest(items=[[4, 5, 6, 7]])) == 4,
        or_tail_len(Wrap(items=[])) == -1, or_tail_len(Nest(items=[[1], [2]])) == -1, or_tail_len(Hollow()) == -1,
        or_row_size(Sized(kind="s", n=3)) == 3, or_row_size(Sized(kind="m", n=4)) == 4,
        or_row_size(Sized(kind="l", n=5)) == 0,
        or_open_band(Sized(kind="old", n=2)) == 1, or_open_band(Sized(kind="m", n=2)) == 2,
        or_open_band(Sized(kind="s", n=3)) == 2, or_open_band(Sized(kind="s", n=4)) == 0,
        or_deep_blank(Success(value=Cell(g=Space(), n=0))) == "deep blank",
        or_deep_blank(Success(value=Cell(g=Tab(), n=1))) == "deep blank",
        or_deep_blank(Error(message="blank")) == "deep blank",
        or_deep_blank(Success(value=Cell(g=Space(), n=2))) == "deep other",
        or_deep_blank(Success(value=Cell(g=Letter(c="x"), n=9))) == "deep letter",
        or_deep_blank(Success(value=Cell(g=Letter(c="y"), n=9))) == "deep letter",
        or_deep_blank(Success(value=Cell(g=Letter(c="z"), n=9))) == "deep other",
        or_deep_blank(Error(message="other")) == "deep other",
    ])

test("or-patterns cover Results, payloads, lists, rows and three-level nests", or_structure_case)

def or_first_wins(t):
    match t:
        case Tri(x, 0, _) | Tri(_, x, 0):
            return x
        case _:
            return -1

def or_pick_call(r):
    match r:
        case Success(f) | Error(f):
            return f(10)

def or_adder(t):
    match t:
        case Pt(n, 0) | Pt(0, n):
            return lambda x: wrapAdd(x, n)
        case _:
            return lambda x: x

def or_binding_case():
    # The outer binding survives selection and application of the arm closure.
    n = 100

    return checkAll("or-pattern bindings follow order and scope", [
        or_first_wins(Tri(a=5, b=0, c=0)) == 5, or_first_wins(Tri(a=3, b=7, c=0)) == 7,
        or_first_wins(Tri(a=5, b=1, c=7)) == -1,
        or_pick_call(Success(value=lambda x: wrapMul(x, 2))) == 20,
        or_pick_call(Error(message=lambda x: wrapAdd(x, 1))) == 11,
        or_axis_pair(4, 0) == 4, n == 100,
        or_adder(Pt(x=3, y=0))(1) == 4, or_adder(Pt(x=0, y=6))(1) == 7, or_adder(Pt(x=2, y=2))(1) == 1,
    ])

test("or-pattern bindings follow order, scope and closures", or_binding_case)
