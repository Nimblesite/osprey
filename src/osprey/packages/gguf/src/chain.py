# osprey: flavor=python
# osprey: namespace=gguf
# osprey: module=Chain
"""Result plumbing shared by this package's modules: continue a parse with the value
of a `Success`, and take the first element of a selection or the error naming it.
"""

__all__ = ["first_or_error", "walk_checked"]


def first_or_error(found, failure):
    """The first element of `found`, or an Error carrying `failure` when it is empty.

    # Examples
    ```osprey
    from gguf import Chain

    def show():
        print(f"{Chain.first_or_error([4, 5], 'none')} {Chain.first_or_error([], 'none')}")

    wrappingArithmetic(show)
    ```
    ```output
    Success(4) Error(none)
    ```
    """
    match listGet(found, 0):
        case Success(value=value):
            return Success(value=value)
        case Error(message=_problem):
            return Error(message=failure)


def walk_checked(count, start, step):
    """`count` steps folded from `start`, or the first problem a step carried.

    `start` is a walk record whose `problem` is "" until a step fails; the fold is
    never recursion per entry, so a huge count costs a loop and not a stack.

    # Examples
    ```osprey
    from gguf import Chain, Reader

    def show():
        print("walk_checked folds a parse walk; see Reader.read for the walk it drives")

    wrappingArithmetic(show)
    ```
    ```output
    walk_checked folds a parse walk; see Reader.read for the walk it drives
    ```
    """
    walk = fold(range(count), start, step)
    if walk.problem != "":
        return Error(message=walk.problem)
    else:
        return Success(value=walk)
