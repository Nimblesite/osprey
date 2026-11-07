# osprey: flavor=python
"""[CI-PROGRAMS-COMPILE] Every tracked Osprey program compiles, or CI fails.

Each tracked osprey.toml is a project, lowered to LLVM IR as a consumer and the
editor see it ([PACKAGE-WORKSPACE]); its --source-graph names its member files.
Every other tracked .osp, .ospml or `# osprey: flavor=python` .py file is a
program lowered on its own. failscompilation/ programs are meant to be rejected:
they never count toward the build (book check-examples holds their goldens).
Run: osprey scripts/ci/check-programs.py --run
"""

WORK = "target/check-programs"
COMPILER = "target/release/osprey"
BATCH = 16
PYTHON_HEADER = "# osprey: flavor=python"
REJECTED = "/failscompilation/"
SOURCE_KEY = "\"source\":\""


class Job:
    path: str
    command: str
    log: str


class Kept:
    segments: list[str]
    skip: int


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


def run(command):
    match spawnProcess(command, ignore):
        case Success(value=handle):
            code = awaitProcess(handle)
            cleanupProcess(handle)
            return code
        case Error():
            return -1


def text(path):
    return unwrap_or(readFile(path), "")


def project_job(index, directory):
    log = f"{WORK}/project-{index}.log"
    graph = f"{COMPILER} '{directory}' --source-graph > {WORK}/graph-{index}.json 2>/dev/null"
    return Job(path=directory, command=f"{graph}; {COMPILER} '{directory}' --llvm --quiet --no-warnings > {log} 2>&1", log=log)


def file_job(index, path):
    log = f"{WORK}/file-{index}.log"
    return Job(path=path, command=f"{COMPILER} '{path}' --llvm --quiet --no-warnings > {log} 2>&1", log=log)


def launch(job):
    match spawnProcess(job.command, ignore):
        case Success(value=handle):
            return handle
        case Error():
            return -1


def settle(handle):
    code = awaitProcess(handle) if handle >= 0 else -1
    cleanupProcess(handle)
    return code


def batch(jobs, start):
    handles = toList(map(range(start, min_int(start + BATCH, len(jobs))), lambda i: launch(unwrap_or(listGet(jobs, i), Job(path="", command="false", log="")))))
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


def graph_members(root, index):
    parts = filterList(unwrap_or(split(text(f"{WORK}/graph-{index}.json"), SOURCE_KEY), []), lambda p: not startsWith(p, "{"))
    return mapList(parts, lambda p: drop(normalize(take(p, unwrap_or(indexOf(p, "\""), 0))), len(root)))


def inside_any(projects, path):
    return foldList(projects, False, lambda found, p: found or startsWith(path, p + "/"))


def is_program(path):
    if contains("/" + path, REJECTED):
        return False
    elif endsWith(path, ".py"):
        return startsWith(text(path), PYTHON_HEADER)
    else:
        return True


def reason(job):
    return trim(text(job.log))


def report(jobs, codes):
    bad = filterList(toList(map(range(0, len(jobs)), lambda i: i)), lambda i: unwrap_or(listGet(codes, i), -1) != 0)
    lines_out = mapList(bad, lambda i: describe(unwrap_or(listGet(jobs, i), Job(path="?", command="", log=""))))
    return join(lines_out, "") + f"{len(jobs)} programs, {len(bad)} failed"


def describe(job):
    return f"FAIL {job.path}\n    {unwrap_or(replace(reason(job), chr_newline(), chr_newline() + '    '), reason(job))}\n"


def chr_newline():
    return "\n"


def main_report():
    setup = run(f"mkdir -p {WORK} && rm -f {WORK}/* && git ls-files '*osprey.toml' > {WORK}/manifests.txt && git ls-files '*.osp' '*.ospml' '*.py' > {WORK}/sources.txt && pwd > {WORK}/root.txt")
    return check_all() if setup == 0 else f"setup failed with exit {setup}, 1 failed"


def check_all():
    root = normalize(trim(text(f"{WORK}/root.txt"))) + "/"
    projects = mapList(lines(text(f"{WORK}/manifests.txt")), lambda m: drop(m, 0) if m == "osprey.toml" else take(m, len(m) - len("/osprey.toml")))
    project_jobs = toList(map(range(0, len(projects)), lambda i: project_job(i, unwrap_or(listGet(projects, i), ""))))
    project_codes = run_all(project_jobs)
    members = foldList(toList(map(range(0, len(projects)), lambda i: i)), [], lambda acc, i: listConcat(acc, graph_members(root, i)))
    files = filterList(lines(text(f"{WORK}/sources.txt")), lambda f: is_program(f) and not listContains(members, f) and not inside_any(projects, f))
    file_jobs = toList(map(range(0, len(files)), lambda i: file_job(i, unwrap_or(listGet(files, i), ""))))
    return report(listConcat(project_jobs, file_jobs), listConcat(project_codes, run_all(file_jobs)))


def checked():
    with halting:
        with arithmetic:
            return main_report()


print(checked())
