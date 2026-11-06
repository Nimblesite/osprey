# osprey: flavor=python
# Osprey in one screen: algebraic effects, fibers, unions, HM inference.
# The SAME account() runs in two worlds; only the installed handler differs.


@effect
class Console:
    def emit(line: str) -> None: ...


@effect
class Ledger:
    def post(balance: int, amount: int) -> int: ...


def account():
    """account() only performs effects: it never learns whether the ledger is real."""
    perform(Console.emit("open account"))
    after_deposit = perform(Ledger.post(0, 100))
    perform(Console.emit(f"deposit 100  → balance {after_deposit}"))
    after_more = perform(Ledger.post(after_deposit, 250))
    perform(Console.emit(f"deposit 250  → balance {after_more}"))
    after_draw = perform(Ledger.post(after_more, -90))
    perform(Console.emit(f"withdraw 90  → balance {after_draw}"))
    return after_more


@handler(Console)
class teller:
    def emit(line):
        print(f"  💸 {line}")


@handler(Ledger)
class books:
    """The real ledger books every posting."""
    def post(balance, amount):
        return balance + amount


def real_world():
    """World A: the real ledger."""
    return teller(lambda: books(account))


@handler(Console)
class sandbox:
    def emit(line):
        print(f"  🧪 [dry-run] {line}")


@handler(Ledger)
class frozen:
    def post(_balance, _amount):
        return 0


def dry_run():
    """World B: same code, a frozen compliance mock. Every post is a no-op."""
    return sandbox(lambda: frozen(account))


def even(x):
    return x % 2 == 0


def sq(x):
    return x * x


def crunch(n):
    """Pure pipeline: sum of squares of the evens in [1, n). No loops, no mutation."""
    return fold(map(filter(range(1, n), even), sq), 0, lambda a, b: a + b)


@union
class Tier:
    """Exhaustive match over a union: drop a case and it won't compile."""
    class Epic:
        pass
    class Solid:
        pass
    class Starter:
        pass


def tier(score):
    if score >= 2000:
        return Epic()
    elif score >= 500:
        return Solid()
    else:
        return Starter()


def badge(t):
    match t:
        case Epic():
            return "🟣 EPIC"
        case Solid():
            return "🔵 SOLID"
        case Starter():
            return "🟢 STARTER"


def show_worlds():
    """ACT 1: the same account() body runs against a real ledger and a dry run."""
    print("OSPREY FEATURE TOUR\n══════════════════════════════════════\nACT 1 · algebraic effects — same code, two worlds")
    real = real_world()
    print(f"  ↳ realWorld() returned {real}")
    mock = dry_run()
    print(f"  ↳ dryRun()   returned {mock}")


def show_fleet():
    """ACT 2: each crunch() runs in its own fiber; awaiting in order keeps the report deterministic."""
    print("══════════════════════════════════════\nACT 2 · fibers compute functional pipelines in parallel")
    fa = spawn(lambda: crunch(10))
    fb = spawn(lambda: crunch(20))
    fc = spawn(lambda: crunch(40))
    ra = await_fiber(fa)
    rb = await_fiber(fb)
    rc = await_fiber(fc)
    print(f"  Σeven² <10  = {ra}  {badge(tier(ra))}\n  Σeven² <20  = {rb}  {badge(tier(rb))}\n  Σeven² <40  = {rc}  {badge(tier(rc))}\n══════════════════════════════════════\ntotal {ra + rb + rc}  ·  fleet {badge(tier(ra + rb + rc))}")


def mega_showcase_case():
    """Replays both effect worlds and the fiber-backed scoring pipeline."""
    real_result = real_world()
    mock_result = dry_run()
    fiber_result = await_fiber(spawn(lambda: crunch(12)))
    score_a = crunch(10)
    score_b = crunch(20)
    score_c = crunch(40)
    return checkAll("effect worlds and functional scoring state", [
        real_result == 350,
        mock_result == 0,
        even(2),
        not even(3),
        sq(9) == 81,
        crunch(1) == 0,
        score_a == 120,
        score_b == 1140,
        score_c == 9880,
        fiber_result == 220,
        score_a + score_b + score_c == 11140,
        badge(tier(499)) == "🟢 STARTER",
        badge(tier(500)) == "🔵 SOLID",
        badge(tier(1999)) == "🔵 SOLID",
        badge(tier(2000)) == "🟣 EPIC",
        badge(Starter()) == "🟢 STARTER",
        badge(Solid()) == "🔵 SOLID",
        badge(Epic()) == "🟣 EPIC",
    ])


@handler(Arith)
class fault_sticky:
    """One overflow policy covers ledger arms, helpers and spawned pipelines."""
    def overflow(_op, lhs, _rhs, _width):
        return lhs


fault_sticky(show_worlds)
fault_sticky(show_fleet)
test("mega showcase preserves effect, fiber, and tier state", lambda: fault_sticky(mega_showcase_case))
