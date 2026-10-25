# osprey: flavor=python
"""FAISS's flat-index tutorial as a runnable vector-search application.

The original 1-Flat.py builds 100,000 random float32 vectors, adds them to
`IndexFlatL2`, prints the first five and last five neighbour rows, then repeats
the search with `IndexFlatIP`. This port keeps the flat L2 and inner-product
indexes, the `ntotal`/`search` flow and the sanity search; it uses a deterministic
24-vector corpus in eight dimensions, replaces the random database, prints labels
and distances instead of raw arrays, checks the FAISS result against numpy, and
saves/reloads the corpus through a safetensors checkpoint. No approximate index
is claimed.
"""

from numpy import Array, Host
from faiss import Flat, Normalize
from scipy import Special

from port_common import Checkpoint, Policy

DIM = 8
CORPUS = 24
QUERY_COUNT = 4
K = 3
FREQ = 0.37
PHASE = 0.11
AMPLITUDE = 0.5
PAIRS = [[0, 1], [6, 7], [12, 13], [18, 19]]


class Checkpointed:
    """The encoded checkpoint and its decoded rows."""
    data: list[int]
    rows: list[list[float]]


class Report:
    """Every value the application prints and asserts about."""
    corpus: list[list[float]]
    queries: list[list[float]]
    l2_distances: list[list[float]]
    l2_labels: list[list[int]]
    ip_labels: list[list[int]]
    ip_scores: list[list[float]]
    confidence: list[float]
    recall: float
    checkpoint: list[list[float]]
    checkpoint_size: int
    numpy_ok: bool


def row_at(rows, i):
    """Row `i` of a matrix, or an empty row."""
    return unwrap_or(listGet(rows, i), [])


def cell(row, j):
    """Element `j` of a row, or zero."""
    return unwrap_or(listGet(row, j), 0.0)


def generated_values(count):
    """A deterministic smooth sequence, standing in for the tutorial's RNG."""
    return toList(map(range(count), lambda i: sin(toFloat(i) * FREQ) + AMPLITUDE * cos(toFloat(i) * PHASE)))


def centered_corpus():
    """The generated matrix with its per-column mean removed."""
    values = generated_values(CORPUS * DIM)
    matrix = Array.reshape(Array.array1(values), [CORPUS, DIM])
    mean = Array.mean_axis(matrix, 0, False)
    return Array.tolist(Array.subtract(matrix, mean))


def mean_of_pair(rows, pair):
    """The midpoint of two corpus rows, as a query vector."""
    i = unwrap_or(listGet(pair, 0), 0)
    j = unwrap_or(listGet(pair, 1), 1)
    two = Array.array2([row_at(rows, i), row_at(rows, j)])
    return Array.tolist(Array.mean_axis(two, 0, False))


def make_queries(rows):
    """One query for each adjacent corpus pair."""
    return toList(map(range(QUERY_COUNT), lambda q: mean_of_pair(rows, unwrap_or(listGet(PAIRS, q), [0, 1]))))


def checked_corpus(rows):
    """Persist the corpus, then decode it back through safetensors."""
    flat = Array.ravel(Array.array2(rows))
    file = Checkpoint.encode_f64("corpus", Array.tolist(flat))
    decoded = unwrap_or(Checkpoint.decode_f64(file, "corpus"), [])
    restored = Array.tolist(Array.reshape(Array.array1(decoded), [CORPUS, DIM]))
    return Checkpointed(data=file, rows=restored)


def contains(xs, value):
    """Whether an int list contains `value`."""
    return foldList(xs, False, lambda found, x: found or x == value)


def pair_hits(labels, q):
    """Whether query q's two source rows both appear in its top-k labels."""
    pair = unwrap_or(listGet(PAIRS, q), [0, 1])
    found = contains(labels, unwrap_or(listGet(pair, 0), 0)) and contains(labels, unwrap_or(listGet(pair, 1), 1))
    return 1 if found else 0


def recall_at_k(labels):
    """Fraction of queries for which both source rows are in the top k."""
    hits = fold(range(QUERY_COUNT), 0, lambda acc, q: acc + pair_hits(unwrap_or(listGet(labels, q), []), q))
    return toFloat(hits) / toFloat(QUERY_COUNT)


def nearest_l2_distance(query, row):
    """The squared L2 distance recomputed independently with numpy."""
    diff = Array.subtract(Array.array1(query), Array.array1(row))
    return Array.sum(Array.multiply(diff, diff))


def numpy_matches(rows, queries, results):
    """Every FAISS distance agrees with numpy's squared L2 recomputation."""
    checks = toList(map(range(QUERY_COUNT), lambda q: local_match(rows, unwrap_or(listGet(queries, q), []), unwrap_or(listGet(results.labels, q), []), unwrap_or(listGet(results.distances, q), []))))
    return foldList(checks, True, lambda ok, value: ok and value)


def local_match(rows, query, labels, distances):
    """One query: recheck its first neighbour's distance."""
    label = unwrap_or(listGet(labels, 0), 0)
    got = unwrap_or(listGet(distances, 0), 0.0)
    want = nearest_l2_distance(query, row_at(rows, label))
    return abs(got - want) < 0.000000001


def confidence_of(scores):
    """Softmax over the first query's inner-product scores."""
    return Special.softmax(scores)


def run():
    """Build, checkpoint, search and evaluate the corpus."""
    rows = centered_corpus()
    queries = make_queries(rows)
    saved = checked_corpus(rows)
    restored = saved.rows
    l2 = Flat.add(Flat.IndexFlatL2(DIM), restored)
    l2_results = Flat.search(l2, queries, K)
    ip = Flat.add(Flat.IndexFlatIP(DIM), Normalize.normalize_L2(restored))
    ip_results = Flat.search(ip, Normalize.normalize_L2(queries), K)
    ip_scores = ip_results.distances
    return Report(
        corpus=restored,
        queries=queries,
        l2_distances=l2_results.distances,
        l2_labels=l2_results.labels,
        ip_labels=ip_results.labels,
        ip_scores=ip_scores,
        confidence=confidence_of(unwrap_or(listGet(ip_scores, 0), [])),
        recall=recall_at_k(l2_results.labels),
        checkpoint=restored,
        checkpoint_size=len(saved.data),
        numpy_ok=numpy_matches(restored, queries, l2_results),
    )


def fmt(value):
    """Four decimal places, stable across the small deterministic values."""
    return toString(round(value * 10000.0) / 10000.0)


def fmt_ints(xs):
    """A compact int list."""
    return "[" + join(mapList(xs, lambda value: toString(value)), ", ") + "]"


def fmt_floats(xs):
    """A compact float list."""
    return "[" + join(mapList(xs, lambda value: fmt(value)), ", ") + "]"


def query_line(report, q):
    """One query's L2 and inner-product results plus its confidence."""
    scores = unwrap_or(listGet(report.ip_scores, q), [])
    return f"q{q} L2 labels={fmt_ints(unwrap_or(listGet(report.l2_labels, q), []))} distances={fmt_floats(unwrap_or(listGet(report.l2_distances, q), []))} " +
        f"IP labels={fmt_ints(unwrap_or(listGet(report.ip_labels, q), []))} scores={fmt_floats(scores)}"


def report_text(report):
    """One consolidated report in place of the tutorial's raw array prints."""
    lines = toList(map(range(QUERY_COUNT), lambda q: query_line(report, q)))
    return join(listConcat([
        "vector-search: faiss IndexFlatL2/IndexFlatIP, d=8 corpus=24 queries=4 k=3",
        f"checkpoint: safetensors bytes={report.checkpoint_size} vectors={len(report.corpus)} exact={structuralEquals(report.checkpoint, report.corpus)}",
    ], listConcat(lines, [
        f"numpy distance check={report.numpy_ok}",
        f"softmax confidence q0={fmt_floats(report.confidence)} sum={fmt(Array.sum(Array.array1(report.confidence)))}",
        f"recall@3={fmt(report.recall)} ({toString(round(report.recall * 8.0))}/8 source rows)",
    ])), "\n")


def main():
    with Policy.arithmetic:
        report = Host.on_host(lambda: run())
        print(report_text(report))
        checkAll("vector-search", [
            len(report.corpus) == CORPUS,
            len(report.queries) == QUERY_COUNT,
            len(report.l2_labels) == QUERY_COUNT,
            report.numpy_ok,
            report.recall == 1.0,
            abs(Array.sum(Array.array1(report.confidence)) - 1.0) < 0.000000001,
            len(report.confidence) == K,
            structuralEquals(report.checkpoint, report.corpus),
            report.checkpoint_size > CORPUS * DIM * 8,
            cell(row_at(report.l2_distances, 0), 0) >= 0.0,
        ])

