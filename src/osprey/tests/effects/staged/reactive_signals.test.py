# osprey: flavor=python
# [FLAVOR-PYTHON-HANDLER] [STAGE-SIGNALS-CACHE] Exercise cached values and callback evaluation against the shared golden.

@static_effect
class Signal[T]:
    def read() -> T: ...

class Count:
    value: int

class Cursor:
    at: int

def count():
    return perform(Signal[Count].read()).value

def cursor():
    return perform(Signal[Cursor].read()).at

def underCount(n, work):
    @static_handler(Signal[Count])
    class read_count:
        def read():
            return Count(value=n)
    return read_count(work)

def underSignals(n, at, work):
    @static_handler(Signal[Cursor])
    class read_cursor:
        def read():
            return Cursor(at=at)
    return underCount(n, lambda: read_cursor(work))

class Views[T]:
    counter: T
    cursor: T
    status: T
    footer: T

def counterLabel():
    return f"count={count()}"

def cursorLabel():
    return f"cursor={cursor()}"

def status():
    return f"{counterLabel()}/{cursorLabel()}"

def footer():
    return "ready"

@requires(tail="e")
def relay(callback):
    return callback()

def branch(flag):
    if flag:
        return counterLabel()
    else:
        return cursorLabel()

def tracksDeps(deps, reference):
    return join(deps, ";") == join(reference, ";")

def snapshotViews():
    return Views(counter=signalSnapshot(counterLabel), cursor=signalSnapshot(cursorLabel), status=signalSnapshot(status), footer=signalSnapshot(footer))

def initial(n):
    return lambda at: underSignals(n, at, snapshotViews)

def changedViews(old, changed):
    return Views(counter=signalRefresh(old.counter, changed, counterLabel), cursor=signalRefresh(old.cursor, changed, cursorLabel), status=signalRefresh(old.status, changed, status), footer=signalRefresh(old.footer, changed, footer))

def refreshAll(old):
    return lambda changed: lambda n: lambda at: underSignals(n, at, lambda: changedViews(old, changed))

def snapshotOf(now):
    return underCount(2, lambda: signalSnapshot(now))

def refreshedFrom(old, later):
    return underCount(4, lambda: signalRefresh(old, signalDependencies(count), later))

def rebuiltOnce(first, next):
    return first.revision == 1 and next.revision == 2

def selectiveRebuild():
    first = initial(1)(2)
    changedCount = refreshAll(first)(signalDependencies(count))(7)(9)
    changedCursor = refreshAll(changedCount)(signalDependencies(cursor))(8)(4)
    batch = refreshAll(changedCursor)(signalDependencies(status))(3)(6)
    idle = refreshAll(batch)([])(99)(99)
    return checkAll("selective reactive views", [
        structuralEquals([first.status.value, changedCount.counter.value], ["count=1/cursor=2", "count=7"]),
        structuralEquals([changedCount.cursor.value, changedCount.status.value], ["cursor=2", "count=7/cursor=9"]),
        structuralEquals([changedCursor.counter.value, changedCursor.cursor.value], ["count=7", "cursor=4"]),
        structuralEquals([changedCursor.status.value, batch.counter.value], ["count=8/cursor=4", "count=3"]),
        structuralEquals([batch.cursor.value, idle.footer.value], ["cursor=6", "ready"]),
        structuralEquals([changedCount.footer.revision, changedCount.counter.revision], [1, 2]),
        structuralEquals([changedCount.cursor.revision, batch.status.revision], [1, 4]),
        structuralEquals([idle.status.revision, idle.footer.revision], [4, 1]),
        idle.status.value == batch.status.value
    ])


def conservativeBranches():
    first = underSignals(1, 2, lambda: signalSnapshot(lambda: branch(True)))
    changed = underSignals(5, 9, lambda: signalRefresh(first, signalDependencies(cursor), lambda: branch(True)))
    return checkAll("may-read includes an inactive branch", [
        structuralEquals([len(first.dependencies), changed.revision], [2, 2]),
        structuralEquals([first.value, changed.value], ["count=1", "count=5"])
    ])


def callbackReplacementAndUnknown():
    first = underCount(1, lambda: signalSnapshot(counterLabel))
    replacement = underSignals(0, 8, lambda: signalRefresh(first, [], cursorLabel))
    unknown = underCount(3, lambda: relay(lambda: signalSnapshot(counterLabel)))
    return checkAll("callback replacement updates dependencies", [
        structuralEquals([replacement.value, unknown.value], ["cursor=8", "count=3"]),
        replacement.revision == 2,
        tracksDeps(replacement.dependencies, signalDependencies(cursorLabel))
    ])


def rowsStayOpen(callback):
    return signalSnapshot(callback)

def unknownRefresh(old):
    return lambda callback: signalRefresh(old, [], callback)

def unresolvedCallbacksRebuild():
    first = rowsStayOpen(lambda: "first")
    next = unknownRefresh(first)(lambda: "next")
    return checkAll("unknown dependencies invalidate conservatively", [
        structuralEquals([join(first.dependencies, ";"), first.value, next.value], ["<unknown>", "first", "next"]),
        next.revision == 2
    ])


def keptValues():
    return [counterLabel(), "kept"]

def managedValues():
    first = snapshotOf(keptValues)
    next = refreshedFrom(first, keptValues)
    return checkAll("snapshots own managed values independently", [
        structuralEquals([join(first.value, ","), join(next.value, ",")], ["count=2,kept", "count=4,kept"]),
        rebuiltOnce(first, next)
    ])

@effect
class Audit:
    def note(text: str) -> None: ...

@handler(Audit)
class log_audit:
    def note(text):
        print(f"audit {text}")

def tracked():
    perform(Audit.note("build;"))
    return counterLabel()

def factory():
    perform(Audit.note("factory;"))
    return tracked

def evaluationAndNoOp():
    with log_audit:
        first = underCount(2, lambda: signalSnapshot(factory()))
        idle = underCount(9, lambda: signalRefresh(first, [], factory()))
        metadata = signalDependencies(factory())
        return checkAll("arguments run once and clean caches skip their callback", [
            structuralEquals([first.value, idle.value], ["count=2", "count=2"]),
            idle.revision == 1, tracksDeps(metadata, signalDependencies(count))
        ])

def unresolvedChangesRebuild():
    first = signalSnapshot(lambda: "first")
    next = signalRefresh(first, ["<unknown>"], lambda: "next")
    return checkAll("unknown changes invalidate conservatively", [
        structuralEquals([first.value, next.value], ["first", "next"]),
        next.revision == 2
    ])

def allowedLabel():
    return Success(value=counterLabel())

def deniedLabel():
    return Error(message="denied")

def answerOf(result):
    match result:
        case Success(value=value):
            return value
        case Error(message=message):
            return message

def cachedResultsKeepBothVariants():
    first = snapshotOf(allowedLabel)
    next = refreshedFrom(first, deniedLabel)
    return checkAll("cached results preserve success and error payloads", [
        structuralEquals([answerOf(first.value), answerOf(next.value)], ["count=2", "denied"]),
        rebuiltOnce(first, next)
    ])

def capturedLabel():
    saved = counterLabel()
    return lambda: saved

def callableSnapshotsKeepTheirCapturedValues():
    first = snapshotOf(capturedLabel)
    idle = underCount(9, lambda: signalRefresh(first, [], capturedLabel))
    next = refreshedFrom(idle, capturedLabel)
    return checkAll("callable snapshots own their captured values", [
        first.value() == "count=2", idle.value() == "count=2", next.value() == "count=4",
        first.revision == 1, idle.revision == 1, next.revision == 2
    ])

class Region:
    region: str
    dependencies: list[str]
    revision: int
    json: str

class Blank:
    dependencies: list[str]
    revision: int
    value: str

class Row:
    revision: int
    value: str

def describeSnapshot(next):
    return f"{next.value}@{next.revision}"

def named(key, next):
    return Region(region=key, dependencies=next.dependencies, revision=next.revision, json=next.value)

def refreshedBlank():
    old = Blank(dependencies=["<unknown>"], revision=0, value="")
    return signalRefresh(old, [], counterLabel)

def describedBlank(n):
    return underCount(n, lambda: describeSnapshot(refreshedBlank()))

def pickRow(cached, flag):
    row = Row(revision=1, value="fresh") if flag else cached
    return describeSnapshot(row)

def refreshArgumentsKeepTheirLayout():
    first = snapshotOf(counterLabel)
    direct = describedBlank(3)
    built = underCount(3, lambda: named("list", refreshedBlank()))
    kept = underCount(9, lambda: named("kept", signalRefresh(first, [], counterLabel)))
    bound = describedBlank(4)
    snapshot = underCount(5, lambda: describeSnapshot(signalSnapshot(counterLabel)))
    print(f"refresh arguments: {direct} {built.region}={built.json}@{built.revision} {kept.region}={kept.json}@{kept.revision} {bound} {snapshot} {describeSnapshot(first)}")
    return checkAll("refreshed snapshots keep their record layout as call arguments", [
        structuralEquals([direct, built.region, built.json, kept.region], ["count=3@1", "list", "count=3", "kept"]),
        structuralEquals([kept.json, bound, snapshot], ["count=2", "count=4@1", "count=5@1"]),
        describeSnapshot(first) == "count=2@1",
        structuralEquals([pickRow(Row(revision=9, value="cached"), False), pickRow(Row(revision=9, value="cached"), True)], ["cached@9", "fresh@1"]),
        structuralEquals([built.revision, kept.revision], [1, 1]),
        tracksDeps(built.dependencies, signalDependencies(counterLabel)),
        tracksDeps(kept.dependencies, first.dependencies)
    ])

test("reactive views rebuild selectively through helpers", selectiveRebuild)
test("inactive branches remain possible dependencies", conservativeBranches)
test("changing a callback updates its dependency set", callbackReplacementAndUnknown)
test("unknown rows rebuild rather than miss a change", unresolvedCallbacksRebuild)
test("old and new snapshots keep their managed values", managedValues)
test("arguments run once while clean computations stay cached", evaluationAndNoOp)
test("unknown changes cannot retain a stale snapshot", unresolvedChangesRebuild)
test("cached results retain whole variants across rebuilds", cachedResultsKeepBothVariants)
test("cached callables retain their captured values across rebuilds", callableSnapshotsKeepTheirCapturedValues)
test("refreshed snapshots keep their layout as call arguments", refreshArgumentsKeepTheirLayout)
