# osprey: flavor=python
# osprey: namespace=gguf
# osprey: module=Chain
"""Result plumbing shared by this package's modules: continue a parse with the value
of a `Success`, and take the first element of a selection or the error naming it.
"""

__all__ = ["then", "first_or_error", "walk_checked"]


def then(result, continue_with):
    """`continue_with` applied to `result`'s value, or `result`'s error unchanged."""
    match result:
        case Success(value=value):
            return continue_with(value)
        case Error(message=message):
            return Error(message=message)


def first_or_error(found, failure):
    """The first element of `found`, or an Error carrying `failure` when it is empty."""
    match listGet(found, 0):
        case Success(value=value):
            return Success(value=value)
        case Error(message=_problem):
            return Error(message=failure)


def walk_checked(count, start, step):
    """`count` steps folded from `start`, or the first problem a step carried.

    `start` is a walk record whose `problem` is "" until a step fails; the fold is
    never recursion per entry, so a huge count costs a loop and not a stack."""
    walk = fold(range(count), start, step)
    if walk.problem != "":
        return Error(message=walk.problem)
    else:
        return Success(value=walk)
