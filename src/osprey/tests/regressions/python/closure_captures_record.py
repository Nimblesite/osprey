# osprey: flavor=python
"""A record built in each iteration of a fused `map` loop and read through a
lambda that captures it leaked under ARC: the captured owner was registered in
the frame beneath the innermost one, which inside a loop body with no
statement frame is the function frame, so every iteration overwrote the
previous record's slot and all but the last were never released. Owners now
register in the innermost scope frame, the loop body, which releases them per
iteration. Found by the @osprey/tokenizers port suite. [GC-ARC-PERCEUS]"""


class Rec:
    xs: list[int]


def f(r):
    return toList(map(range(len(r.xs)), lambda i: unwrap_or(listGet(r.xs, i), 0)))


def main():
    return print(toString(len(toList(map(range(13), lambda i: f(Rec(xs=[1, 2, 3])))))))
