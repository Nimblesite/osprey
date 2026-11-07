# osprey: flavor=python
"""[CI-PROGRAMS-COMPILE] Every tracked Osprey program compiles, or CI fails.

Each tracked osprey.toml is a project, lowered to LLVM IR as a consumer and the
editor see it ([PACKAGE-WORKSPACE]); its --source-graph names its member files.
Every other tracked .osp, .ospml or Python-flavour .py file is a program lowered
on its own. Files under a project that fails are covered by its FAIL line.
Left out, each checked elsewhere: failscompilation/ (must be rejected: the
book's .expectederror goldens and osprey-cli's examples_compile), and the
registry data (sample-registry/, e2e/consumer/, fixtures/manifests/), which
resolves only through a registry and is built by make _test_registry.
Run: OSPREY_BIN=<compiler> osprey scripts/ci/check-programs.py --run
"""

WORK = "target/check-programs"
COMPILER = "\"${OSPREY_BIN:-target/release/osprey}\""
BATCH = 16
NEWLINE = "\n"
EXCLUDED = ["/failscompilation/", "/sample-registry/", "/e2e/consumer/", "/fixtures/manifests/"]
PYTHON_MARKERS = ["# osprey: flavor=python", "#osprey: flavor=python"]
SOURCE_KEY = "\"source\":\""
SOURCES_KEY = "\"sources\":["


class Job:
    path: str
    command: str
    log: str


class Kept:
    segments: list[str]
    skip: int


class Header:
    open: bool
    found: bool


@effect
class Fault:
    """An arithmetic fault ends the check: its arm answers the whole run."""
    @control
    def halt(message: str) -> int: ...


@handler(Fault)
class halting:
    def halt(fault):
        return f"{fault}: the check stopped, 1 failed"


@handler(Arith)
class arithmetic:
    def overflow(op, l, r, _):
        return perform(Fault.halt(f"integer overflow: {l} {op} {r}"))

    def divideByZero(op, l):
        match perform(Fault.halt(f"division by zero: {l} {op} 0")):
            case _:
                return l

    def remainderByZero(l):
        return perform(Fault.halt(f"remainder by zero: {l} % 0"))


def ignore(_handle, _kind, _text):
    pass


def text(path):
    return unwrap_or(readFile(path), "")


def project_job(index, directory):
    log = f"{WORK}/project-{index}.log"
    graph = f"{COMPILER} '{directory}' --source-graph > {WORK}/graph-{index}.json 2>> {log}"
    return Job(path=directory, command=f"{graph}; {COMPILER} '{directory}' --llvm --quiet --no-warnings >> {log} 2>&1", log=log)


def file_job(index, path):
    log = f"{WORK}/file-{index}.log"
    return Job(path=path, command=f"{COMPILER} '{path}' --llvm --quiet --no-warnings > {log} 2>&1", log=log)


def launch(command):
    match spawnProcess(command, ignore):
        case Success(value=handle):
            return handle
        case Error():
            return -1


def settle(handle):
    code = awaitProcess(handle) if handle >= 0 else -1
    cleanupProcess(handle)
    return code


def run(command):
    return settle(launch(command))


def job_at(jobs, i):
    return unwrap_or(listGet(jobs, i), Job(path="?", command="false", log=""))


def batch(jobs, start):
    handles = toList(map(range(start, min_int(start + BATCH, len(jobs))), lambda i: launch(job_at(jobs, i).command)))
    return mapList(handles, settle)


def min_int(a, b):
    return a if a < b else b


def run_all(jobs):
    return fold(range(0, intDiv(len(jobs) + BATCH - 1, BATCH)), [], lambda codes, k: listConcat(codes, batch(jobs, k * BATCH)))


def keep(kept, segment):
    if segment == "..":
        return Kept(segments=kept.segments, skip=kept.skip + 1)
    elif kept.skip > 0:
        return Kept(segments=kept.segments, skip=kept.skip - 1)
    else:
        return Kept(segments=listPrepend(kept.segments, segment), skip=0)


def normalize(path):
    segments = filterList(unwrap_or(split(path, "/"), []), lambda s: s != "" and s != ".")
    return join(foldList(listReverse(segments), Kept(segments=[], skip=0), keep).segments, "/")


def after(haystack, key):
    parts = unwrap_or(split(haystack, key), [])
    return toList(map(range(1, len(parts)), lambda i: unwrap_or(listGet(parts, i), "")))


def until(part, mark):
    return take(part, unwrap_or(indexOf(part, mark), 0))


def listed_sources(graph):
    entries = foldList(after(graph, SOURCES_KEY), [], lambda acc, p: listConcat(acc, unwrap_or(split(until(p, "]"), ","), [])))
    return mapList(filterList(entries, lambda e: startsWith(e, "\"")), lambda e: until(drop(e, 1), "\""))


def graph_members(root, index):
    graph = text(f"{WORK}/graph-{index}.json")
    paths = listConcat(mapList(after(graph, SOURCE_KEY), lambda p: until(p, "\"")), listed_sources(graph))
    return mapList(paths, lambda p: drop(normalize(p), len(root)))


def under_any(directories, path):
    return foldList(directories, False, lambda found, d: found or startsWith(path, d + "/"))


def excluded(path):
    return foldList(EXCLUDED, False, lambda found, part: found or contains("/" + path, part))


def scan(header, line):
    t = trim(line)
    if not header.open or t == "" or startsWith(t, "//"):
        return header
    elif startsWith(t, "#"):
        return Header(open=True, found=header.found or listContains(PYTHON_MARKERS, t))
    else:
        return Header(open=False, found=header.found)


def is_program(path):
    if excluded(path):
        return False
    elif endsWith(path, ".py"):
        return foldList(lines(text(path)), Header(open=True, found=False), scan).found
    else:
        return True


def describe(job):
    log = trim(text(job.log))
    return f"FAIL {job.path}\n    {unwrap_or(replace(log, NEWLINE, NEWLINE + '    '), log)}\n"


def report(jobs, codes):
    bad = filterList(toList(map(range(0, len(jobs)), lambda i: i)), lambda i: unwrap_or(listGet(codes, i), -1) != 0)
    return join(mapList(bad, lambda i: describe(job_at(jobs, i))), "") + f"{len(jobs)} programs, {len(bad)} failed"


def main_report():
    setup = run(f"mkdir -p {WORK} && rm -f {WORK}/* && git ls-files '*/osprey.toml' > {WORK}/manifests.txt && git ls-files '*.osp' '*.ospml' '*.py' > {WORK}/sources.txt && pwd > {WORK}/root.txt")
    return check_all() if setup == 0 else f"setup failed with exit {setup}, 1 failed"


def check_all():
    root = normalize(trim(text(f"{WORK}/root.txt"))) + "/"
    manifests = filterList(lines(text(f"{WORK}/manifests.txt")), lambda m: not excluded(m))
    projects = mapList(manifests, lambda m: take(m, len(m) - len("/osprey.toml")))
    project_jobs = toList(map(range(0, len(projects)), lambda i: project_job(i, unwrap_or(listGet(projects, i), ""))))
    project_codes = run_all(project_jobs)
    indices = toList(map(range(0, len(projects)), lambda i: i))
    members = foldList(indices, [], lambda acc, i: listConcat(acc, graph_members(root, i)))
    failed = mapList(filterList(indices, lambda i: unwrap_or(listGet(project_codes, i), -1) != 0), lambda i: unwrap_or(listGet(projects, i), ""))
    files = filterList(lines(text(f"{WORK}/sources.txt")), lambda f: is_program(f) and not listContains(members, f) and not under_any(failed, f))
    file_jobs = toList(map(range(0, len(files)), lambda i: file_job(i, unwrap_or(listGet(files, i), ""))))
    return report(listConcat(project_jobs, file_jobs), listConcat(project_codes, run_all(file_jobs)))


def checked():
    with halting:
        with arithmetic:
            return main_report()


print(checked())
