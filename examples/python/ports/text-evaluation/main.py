# osprey: flavor=python
"""Google ROUGE's scorer as a runnable text-evaluation application.

The original `rouge/rouge.py` reads target and prediction files, scores them
with `RougeScorer`, and writes a CSV of per-example and aggregate ROUGE values.
This port keeps the scorer, the requested rouge types, the stemmer option and
the aggregate report; it replaces files and `absl` flags with three fixed
summary candidates, adds corpus BLEU/chrF and WordPiece coverage checks, and
renders the report with a Jinja2 template instead of a CSV. The original
bootstrap aggregator is not ported, so no confidence interval is printed.
"""

from rouge_score import RougeScorer
from sacrebleu import Bleu, Chrf
from tokenizers import Models, Tokenizer
from jinja2 import Render, Value as V

from port_common import Policy

ROUGE_TYPES = ["rouge1", "rouge2", "rougeL", "rougeLsum"]
CLS_ID = 2
SEP_ID = 3
UNK_ID = 1
VOCAB = ["[PAD]", "[UNK]", "[CLS]", "[SEP]", "the", "committee", "approved", "transit", "plan", "after", "months", "of", "debate", "a", "new", "was", "following", "lengthy", "members"]
REFERENCES = [
    "The committee approved the new transit plan after months of debate.",
    "A transit plan won committee approval following lengthy debate.",
    "Committee members approved a new transit plan after debate.",
]
CANDIDATES = [
    ["strong", "The committee approved the transit plan after months of debate."],
    ["short", "A transit plan was approved."],
    ["off-topic", "The weather was sunny and warm all week."],
]
TEMPLATE = """# {{ title }}

| candidate | ROUGE-1 | ROUGE-2 | ROUGE-L | ROUGE-Lsum | BLEU | chrF | tokens | UNK |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
{% for row in rows %}| {{ row[0] }} | {{ row[1] }} | {{ row[2] }} | {{ row[3] }} | {{ row[4] }} | {{ row[5] }} | {{ row[6] }} | {{ row[7] }} | {{ row[8] }} |
{% endfor %}
Corpus BLEU {{ corpus_bleu }} / chrF {{ corpus_chrf }} over {{ reference_count }} references."""


class Coverage:
    """WordPiece token and unknown counts for one candidate."""
    tokens: int
    unknown: int


class Evaluation:
    """One candidate's ROUGE, BLEU, chrF and WordPiece coverage."""
    name: str
    scores: list[float]
    bleu: float
    chrf: float
    tokens: int
    unknown: int


class Run:
    """The candidate evaluations, corpus metrics and rendered markdown."""
    evaluations: list[Evaluation]
    corpus_bleu: float
    corpus_chrf: float
    report: str


def wordpiece_model():
    """The fixed WordPiece vocabulary the coverage check tokenizes with."""
    return Models.build_wordpiece(VOCAB, "[UNK]", "##", 100)


def metric_bleu():
    """Sentence BLEU with sacrebleu's effective order, as the CLI defaults."""
    return Bleu.BLEU(lowercase=True, force=False, tokenize="13a", smooth_method="exp", smooth_value=0.0, max_ngram_order=4, effective_order=True)


def metric_chrf():
    """chrF2 without whitespace, matching sacrebleu's sentence defaults."""
    return Chrf.CHRF(char_order=6, word_order=0, beta=3, lowercase=True, whitespace=True, eps_smoothing=False)


def rouge_fmeasures(scorer, text):
    """The four requested ROUGE types' best-target f-measures, in order."""
    scored = RougeScorer.score_multi(scorer, REFERENCES, text)
    return mapList(scored, lambda item: item.fmeasure)


def token_coverage(model, text):
    """Token count without the two specials, and unknown-token count."""
    encoding = Tokenizer.wordpiece_encode(model, text, CLS_ID, SEP_ID)
    return Coverage(tokens=len(encoding.ids) - 2, unknown=foldList(encoding.ids, 0, lambda acc, token_id: acc + 1 if token_id == UNK_ID else acc))


def evaluate(model, scorer, bleu, chrf, candidate):
    """Every metric for one named candidate."""
    name = unwrap_or(listGet(candidate, 0), "")
    text = unwrap_or(listGet(candidate, 1), "")
    coverage = token_coverage(model, text)
    return Evaluation(
        name=name,
        scores=rouge_fmeasures(scorer, text),
        bleu=Bleu.score_sentence(bleu, text, REFERENCES).score,
        chrf=Chrf.score_sentence(chrf, text, REFERENCES).score,
        tokens=coverage.tokens,
        unknown=coverage.unknown,
    )


def corpus_bleu(bleu, texts):
    """Corpus BLEU with every candidate scored against all three references."""
    return Bleu.score_corpus(bleu, texts, reference_streams(len(texts))).score


def corpus_chrf(chrf, texts):
    """Corpus chrF; `references` is a reference stream per reference."""
    return Chrf.score_corpus(chrf, texts, reference_streams(len(texts))).score


def reference_streams(count):
    """Each reference repeated once per candidate, as corpus scoring expects."""
    return toList(map(range(len(REFERENCES)), lambda r: toList(map(range(count), lambda _c: unwrap_or(listGet(REFERENCES, r), "")))))


def fmt(value):
    """Four decimal places for the report table."""
    return toString(round(value * 10000.0) / 10000.0)


def row_value(evaluation):
    """One Jinja2 table row as typed values."""
    return V.list_([
        V.text(evaluation.name),
        V.text(fmt(unwrap_or(listGet(evaluation.scores, 0), 0.0))),
        V.text(fmt(unwrap_or(listGet(evaluation.scores, 1), 0.0))),
        V.text(fmt(unwrap_or(listGet(evaluation.scores, 2), 0.0))),
        V.text(fmt(unwrap_or(listGet(evaluation.scores, 3), 0.0))),
        V.text(fmt(evaluation.bleu)),
        V.text(fmt(evaluation.chrf)),
        V.text(toString(evaluation.tokens)),
        V.text(toString(evaluation.unknown)),
    ])


def render_report(title, evaluations, corpus_bleu_value, corpus_chrf_value):
    """Render the markdown report, or an explicit failure message."""
    context = {"title": V.text(title), "rows": V.list_(mapList(evaluations, row_value)), "corpus_bleu": V.text(fmt(corpus_bleu_value)), "corpus_chrf": V.text(fmt(corpus_chrf_value)), "reference_count": V.text(toString(len(REFERENCES)))}
    match Render.render(Render.chat_environment(), TEMPLATE, context):
        case Success(value=text):
            return text
        case Error(message=message):
            return "report failed: " + message


def run():
    """Score every candidate, then render the report."""
    model = wordpiece_model()
    scorer = RougeScorer.RougeScorer(rouge_types=ROUGE_TYPES, use_stemmer=True, split_summaries=False)
    bleu = metric_bleu()
    chrf = metric_chrf()
    evaluations = mapList(CANDIDATES, lambda candidate: evaluate(model, scorer, bleu, chrf, candidate))
    texts = mapList(CANDIDATES, lambda candidate: unwrap_or(listGet(candidate, 1), ""))
    corpus_bleu_value = corpus_bleu(bleu, texts)
    corpus_chrf_value = corpus_chrf(chrf, texts)
    return Run(evaluations=evaluations, corpus_bleu=corpus_bleu_value, corpus_chrf=corpus_chrf_value, report=render_report("Evaluation report", evaluations, corpus_bleu_value, corpus_chrf_value))


def eval_at(run, index):
    """One candidate's evaluation, or an empty fallback."""
    return unwrap_or(listGet(run.evaluations, index), Evaluation(name="", scores=[], bleu=0.0, chrf=0.0, tokens=0, unknown=0))


def score_of(run, index, metric):
    """One evaluation's ROUGE field, with a zero fallback."""
    return unwrap_or(listGet(eval_at(run, index).scores, metric), 0.0)


def main():
    with Policy.arithmetic:
        run_value = run()
        print(run_value.report)
        checkAll("text-evaluation", [
            len(run_value.evaluations) == len(CANDIDATES),
            score_of(run_value, 0, 0) > score_of(run_value, 2, 0),
            score_of(run_value, 0, 3) > score_of(run_value, 2, 3),
            eval_at(run_value, 0).bleu > eval_at(run_value, 2).bleu,
            eval_at(run_value, 0).chrf > eval_at(run_value, 2).chrf,
            eval_at(run_value, 0).tokens > 0,
            eval_at(run_value, 2).unknown > 0,
            run_value.corpus_bleu >= 0.0,
            run_value.corpus_bleu <= 100.0,
            run_value.corpus_chrf >= 0.0,
            run_value.corpus_chrf <= 100.0,
            contains(run_value.report, "# Evaluation report"),
            contains(run_value.report, "| strong |"),
            contains(run_value.report, "Corpus BLEU"),
            not contains(run_value.report, "report failed"),
        ])

