"""CPython oracle for @osprey/transformers.

Runs the real transformers 5.20.2 logits processors, warpers and stopping
criteria (torch 2.14.1, float64) and torch's own softmax/log_softmax/argmax/
inverse-CDF sampling over hand-built logits rows, then hands every computed
value to `expected_module.write` to emit
packages/transformers/tests/expected.py ([PORT-TEST-ORACLE]).

The numerical primitives come straight from torch: `torch.softmax(t, dim=-1)`,
`torch.log_softmax(t, dim=-1)`, `torch.argmax(t)` (first maximal index on ties)
and `torch.searchsorted(torch.cumsum(probs, -1), u, right=True)` clamped to the
last index for `multinomial` (torch's inverse-CDF rule). torch's CPU sort is
only stable for short tensors, so TopP/Typical rows keep vocab size <= 16 and
cumulative boundaries away from tie groups; MinP rows keep distinct scores
because torch.topk's tie order is unspecified.
"""
import sys

from expected_module import write

# This script is named transformers.py; drop its own directory from sys.path so
# `import transformers` resolves to the real package, not to this file.
sys.path.pop(0)

import torch
import transformers
from transformers import (
    EpsilonLogitsWarper,
    EtaLogitsWarper,
    ForcedBOSTokenLogitsProcessor,
    ForcedEOSTokenLogitsProcessor,
    MinLengthLogitsProcessor,
    MinNewTokensLengthLogitsProcessor,
    MinPLogitsWarper,
    NoBadWordsLogitsProcessor,
    NoRepeatNGramLogitsProcessor,
    RepetitionPenaltyLogitsProcessor,
    SuppressTokensLogitsProcessor,
    TemperatureLogitsWarper,
    TopKLogitsWarper,
    TopPLogitsWarper,
    TypicalLogitsWarper,
)
from transformers.generation.logits_process import LogitsProcessorList
from transformers.generation.stopping_criteria import (
    EosTokenCriteria,
    MaxLengthCriteria,
    StoppingCriteriaList,
)

torch.set_default_dtype(torch.float64)

NEG_INF = float("-inf")


# ---------------------------------------------------------------------------
# Logits rows: distinct, ties, all-equal, -inf, negatives, extremes.
# ---------------------------------------------------------------------------
R0 = [1.0, 2.0, 3.0, 4.0, 5.0]
R1 = [2.0, 2.0, 2.0, 2.0, 2.0]
R2 = [1.0, 3.0, 1.0, 3.0, 2.0]
R3 = [1.0, NEG_INF, 3.0, NEG_INF, 2.0]
R4 = [-1.0, -2.0, -3.0, -4.0, -5.0]
R5 = [1e300, 1e-300, -1e300, 5.0, 0.0]
R6 = [-0.5, 2.0, -0.5, 2.0, 1.5]
R7 = [0.1, 0.9, 0.4, 0.7, 0.3, 0.8, 0.2, 0.6]
R8 = [0.0, 10.0]
R9 = [3.0, 1.0, NEG_INF, 2.0, 2.0, NEG_INF, 0.5, 3.0, 1.0, 0.5, 2.0, 0.0]
R10 = [NEG_INF, NEG_INF, NEG_INF]


def tensor(values):
    """A float64 torch tensor holding one logits/probability row."""
    return torch.tensor(values, dtype=torch.float64)


def softmax_ref(scores):
    """torch.softmax(scores, dim=-1), the value the port's `softmax` must match."""
    return torch.softmax(tensor(scores), dim=-1).tolist()


def log_softmax_ref(scores):
    """torch.log_softmax(scores, dim=-1), the value the port's `log_softmax` must match."""
    return torch.log_softmax(tensor(scores), dim=-1).tolist()


def argmax_ref(scores):
    """torch.argmax(scores): the first index holding the maximum on ties."""
    return int(torch.argmax(tensor(scores)).item())


def multinomial_ref(probs, u):
    """torch's inverse-CDF rule: the first index whose cumulative probability
    strictly exceeds `u` (`torch.searchsorted` with `right=True`), clamped to
    the last index when none does."""
    cdf = torch.cumsum(tensor(probs), -1)
    return min(int(torch.searchsorted(cdf, u, right=True).item()), len(probs) - 1)


def run1(processor, ids, scores):
    """Apply a single-row processor the way generate does: batch of one."""
    out = processor(torch.tensor([ids]), torch.tensor([scores]))
    return out[0].tolist()


def run1_list(processors, ids, scores):
    """Apply a LogitsProcessorList to one row."""
    out = processors(torch.tensor([ids]), torch.tensor([scores]))
    return out[0].tolist()


# ---------------------------------------------------------------------------
# Math primitives: softmax/log_softmax/argmax/multinomial, all via torch.
# ---------------------------------------------------------------------------
def add_math(C):
    softmax_in = []
    softmax_out = []
    logsoftmax_in = []
    logsoftmax_out = []
    for r in (R0, R1, R2, R3, R4, R5, R6, R7, R8, R10):
        softmax_in.append(r)
        softmax_out.append(softmax_ref(r))
        logsoftmax_in.append(r)
        logsoftmax_out.append(log_softmax_ref(r))
    argmax_in = [R0, R1, R2, R3, R4, R7, R9]
    argmax_out = [argmax_ref(r) for r in argmax_in]
    multi_cases = [
        ([0.2, 0.0, 0.5, 0.3], 0.0),
        ([0.2, 0.0, 0.5, 0.3], 0.2),
        ([0.2, 0.0, 0.5, 0.3], 0.2000001),
        ([0.2, 0.0, 0.5, 0.3], 0.7),
        ([0.2, 0.0, 0.5, 0.3], 0.999999),
        ([1.0, 0.0, 0.0], 0.0),
        ([1.0, 0.0, 0.0], 0.5),
        ([1.0, 0.0, 0.0], 0.99),
        ([0.5, 0.5], 0.0),
        ([0.5, 0.5], 0.5),
        ([0.5, 0.5], 0.9),
        ([0.1] * 10, 0.05),
        ([0.1] * 10, 0.55),
        ([0.1] * 10, 0.9999),
    ]
    C["SOFTMAX_IN"] = softmax_in
    C["SOFTMAX_OUT"] = softmax_out
    C["LOGSOFTMAX_IN"] = logsoftmax_in
    C["LOGSOFTMAX_OUT"] = logsoftmax_out
    C["ARGMAX_IN"] = argmax_in
    C["ARGMAX_OUT"] = argmax_out
    C["MULTI_PROBS"] = [p for p, _ in multi_cases]
    C["MULTI_U"] = [u for _, u in multi_cases]
    C["MULTI_OUT"] = [multinomial_ref(p, u) for p, u in multi_cases]


def add_temperature(C):
    cases = []
    for temp in (0.5, 1.0, 2.0, 0.7):
        for r in (R0, R4, R6, R5):
            p = TemperatureLogitsWarper(temperature=temp)
            cases.append((temp, [], r, run1(p, [], r)))
    C["TEMP_TEMP"] = [t for t, _, _, _ in cases]
    C["TEMP_IDS"] = [[] for _ in cases]
    C["TEMP_SCORES"] = [r for _, _, r, _ in cases]
    C["TEMP_EXPECTED"] = [e for _, _, _, e in cases]


def add_top_k(C):
    cases = []
    for k, m in ((3, 1), (1, 1), (5, 1), (1, 2), (2, 2), (10, 1)):
        for r in (R0, R1, R2, R3, R7):
            p = TopKLogitsWarper(top_k=k, min_tokens_to_keep=m)
            cases.append((k, m, [], r, run1(p, [], r)))
    C["TOPK_K"] = [k for k, _, _, _, _ in cases]
    C["TOPK_MIN"] = [m for _, m, _, _, _ in cases]
    C["TOPK_IDS"] = [[] for _ in cases]
    C["TOPK_SCORES"] = [r for _, _, _, r, _ in cases]
    C["TOPK_EXPECTED"] = [e for _, _, _, _, e in cases]


def add_top_p(C):
    cases = []
    for p, m in ((0.9, 1), (0.5, 1), (1.0, 1), (0.0, 1), (0.6, 2), (0.95, 3)):
        for r in (R0, R1, R2, R4, R6, R7):
            proc = TopPLogitsWarper(top_p=p, min_tokens_to_keep=m)
            cases.append((p, m, [], r, run1(proc, [], r)))
    C["TOPP_P"] = [p for p, _, _, _, _ in cases]
    C["TOPP_MIN"] = [m for _, m, _, _, _ in cases]
    C["TOPP_IDS"] = [[] for _ in cases]
    C["TOPP_SCORES"] = [r for _, _, _, r, _ in cases]
    C["TOPP_EXPECTED"] = [e for _, _, _, _, e in cases]


def add_min_p(C):
    cases = []
    for p, m in ((0.0, 1), (0.5, 1), (0.2, 1), (1.0, 1), (0.1, 2), (0.3, 3)):
        for r in (R0, R4, R6, R7, R8):
            proc = MinPLogitsWarper(min_p=p, min_tokens_to_keep=m)
            cases.append((p, m, [], r, run1(proc, [], r)))
    C["MINP_P"] = [p for p, _, _, _, _ in cases]
    C["MINP_MIN"] = [m for _, m, _, _, _ in cases]
    C["MINP_IDS"] = [[] for _ in cases]
    C["MINP_SCORES"] = [r for _, _, _, r, _ in cases]
    C["MINP_EXPECTED"] = [e for _, _, _, _, e in cases]


def add_typical(C):
    cases = []
    for mass, m in ((0.9, 1), (0.5, 1), (0.1, 1), (0.9, 2)):
        for r in (R0, R2, R4, R6, R7):
            proc = TypicalLogitsWarper(mass=mass, min_tokens_to_keep=m)
            cases.append((mass, m, [], r, run1(proc, [], r)))
    C["TYP_MASS"] = [x for x, _, _, _, _ in cases]
    C["TYP_MIN"] = [m for _, m, _, _, _ in cases]
    C["TYP_IDS"] = [[] for _ in cases]
    C["TYP_SCORES"] = [r for _, _, _, r, _ in cases]
    C["TYP_EXPECTED"] = [e for _, _, _, _, e in cases]


def add_epsilon(C):
    cases = []
    for eps, m in ((0.1, 1), (0.01, 1), (0.5, 1), (0.3, 2)):
        for r in (R0, R4, R6, R7, R8):
            proc = EpsilonLogitsWarper(epsilon=eps, min_tokens_to_keep=m)
            cases.append((eps, m, [], r, run1(proc, [], r)))
    C["EPS_EPS"] = [x for x, _, _, _, _ in cases]
    C["EPS_MIN"] = [m for _, m, _, _, _ in cases]
    C["EPS_IDS"] = [[] for _ in cases]
    C["EPS_SCORES"] = [r for _, _, _, r, _ in cases]
    C["EPS_EXPECTED"] = [e for _, _, _, _, e in cases]


def add_eta(C):
    cases = []
    for eps, m in ((0.05, 1), (0.001, 1), (0.05, 2)):
        for r in (R0, R4, R3, R6, R7):
            proc = EtaLogitsWarper(epsilon=eps, min_tokens_to_keep=m)
            cases.append((eps, m, [], r, run1(proc, [], r)))
    C["ETA_EPS"] = [x for x, _, _, _, _ in cases]
    C["ETA_MIN"] = [m for _, m, _, _, _ in cases]
    C["ETA_IDS"] = [[] for _ in cases]
    C["ETA_SCORES"] = [r for _, _, _, r, _ in cases]
    C["ETA_EXPECTED"] = [e for _, _, _, _, e in cases]


def add_repetition_penalty(C):
    ids_rows = [[0, 1, 2], [0, 1, 0, 1], [2, 3, 2], [0, 5, 2], [1, 1, 1, 3]]
    scores_rows = [R0, R0, [0.0, -1.0, 2.0, -3.0], R0, R6]
    cases = []
    for pen, ign in ((1.2, 0), (2.0, 0), (0.5, 0), (2.0, 2)):
        for ids, scores in zip(ids_rows, scores_rows):
            proc = RepetitionPenaltyLogitsProcessor(penalty=pen, prompt_ignore_length=ign)
            cases.append((pen, ign, ids, scores, run1(proc, ids, scores)))
    C["REPP_PEN"] = [x for x, _, _, _, _ in cases]
    C["REPP_IGNORE"] = [g for _, g, _, _, _ in cases]
    C["REPP_IDS"] = [i for _, _, i, _, _ in cases]
    C["REPP_SCORES"] = [s for _, _, _, s, _ in cases]
    C["REPP_EXPECTED"] = [e for _, _, _, _, e in cases]


def add_no_repeat_ngram(C):
    ids_rows = [
        [1, 2, 3, 1, 2],
        [1, 2, 3, 1],
        [1, 2, 1, 2, 1],
        [1, 1, 1],
        [1, 2, 3],
        [0, 1, 0, 1, 0, 1, 0],
    ]
    scores_rows = [R0, R0, R0, R0, R0, R7]
    cases = []
    for n in (2, 3, 4):
        for ids, scores in zip(ids_rows, scores_rows):
            proc = NoRepeatNGramLogitsProcessor(ngram_size=n)
            cases.append((n, ids, scores, run1(proc, ids, scores)))
    C["NRN_N"] = [n for n, _, _, _ in cases]
    C["NRN_IDS"] = [i for _, i, _, _ in cases]
    C["NRN_SCORES"] = [s for _, _, s, _ in cases]
    C["NRN_EXPECTED"] = [e for _, _, _, e in cases]


def add_min_length(C):
    cases = []
    for ml, eos in ((3, [4]), (5, [4]), (1, [4]), (3, [1, 4])):
        for ids in ([0, 1], [0, 1, 2], [0, 1, 2, 3], [0, 1, 2, 3, 4], [0, 1, 2, 3, 4, 5]):
            proc = MinLengthLogitsProcessor(min_length=ml, eos_token_id=eos)
            cases.append((ml, eos, ids, R2, run1(proc, ids, R2)))
    C["MINL_LEN"] = [ml for ml, _, _, _, _ in cases]
    C["MINL_EOS"] = [e for _, e, _, _, _ in cases]
    C["MINL_IDS"] = [i for _, _, i, _, _ in cases]
    C["MINL_SCORES"] = [s for _, _, _, s, _ in cases]
    C["MINL_EXPECTED"] = [e for _, _, _, _, e in cases]


def add_min_new_tokens(C):
    cases = []
    for skip, new, eos in ((2, 3, [4]), (0, 2, [4]), (3, 1, [4]), (2, 3, [1, 4])):
        for ids in ([0, 1], [0, 1, 2], [0, 1, 2, 3], [0, 1, 2, 3, 4], [0, 1, 2, 3, 4, 5]):
            proc = MinNewTokensLengthLogitsProcessor(
                prompt_length_to_skip=skip, min_new_tokens=new, eos_token_id=eos
            )
            cases.append((skip, new, eos, ids, R2, run1(proc, ids, R2)))
    C["MNT_SKIP"] = [s for s, _, _, _, _, _ in cases]
    C["MNT_NEW"] = [n for _, n, _, _, _, _ in cases]
    C["MNT_EOS"] = [e for _, _, e, _, _, _ in cases]
    C["MNT_IDS"] = [i for _, _, _, i, _, _ in cases]
    C["MNT_SCORES"] = [s for _, _, _, _, s, _ in cases]
    C["MNT_EXPECTED"] = [e for _, _, _, _, _, e in cases]


def add_forced_bos(C):
    cases = []
    for bos in (0, 3):
        for ids in ([7], [7, 8], [7, 8, 9]):
            proc = ForcedBOSTokenLogitsProcessor(bos_token_id=bos)
            cases.append((bos, ids, R0, run1(proc, ids, R0)))
    C["FBOS_BOS"] = [b for b, _, _, _ in cases]
    C["FBOS_IDS"] = [i for _, i, _, _ in cases]
    C["FBOS_SCORES"] = [s for _, _, s, _ in cases]
    C["FBOS_EXPECTED"] = [e for _, _, _, e in cases]


def add_forced_eos(C):
    cases = []
    for ml, eos in ((5, [2]), (4, [2]), (5, [1, 2])):
        for ids in ([0, 1], [0, 1, 2], [0, 1, 2, 3], [0, 1, 2, 3, 4]):
            proc = ForcedEOSTokenLogitsProcessor(max_length=ml, eos_token_id=eos)
            cases.append((ml, eos, ids, R0, run1(proc, ids, R0)))
    C["FEOS_MAX"] = [ml for ml, _, _, _, _ in cases]
    C["FEOS_EOS"] = [e for _, e, _, _, _ in cases]
    C["FEOS_IDS"] = [i for _, _, i, _, _ in cases]
    C["FEOS_SCORES"] = [s for _, _, _, s, _ in cases]
    C["FEOS_EXPECTED"] = [e for _, _, _, _, e in cases]


def add_suppress_tokens(C):
    cases = []
    for supp in ([1, 3], [0], [2, 4]):
        for r in (R0, R2, R4, R6):
            proc = SuppressTokensLogitsProcessor(suppress_tokens=supp)
            cases.append((supp, [], r, run1(proc, [], r)))
    C["SUPP_TOKENS"] = [s for s, _, _, _ in cases]
    C["SUPP_IDS"] = [[] for _ in cases]
    C["SUPP_SCORES"] = [r for _, _, r, _ in cases]
    C["SUPP_EXPECTED"] = [e for _, _, _, e in cases]


def bad_words_ref(bad_words_ids, eos_ids, ids, scores):
    """NoBadWordsLogitsProcessor's intended semantics: bias -inf on the last token
    of a bad-word sequence the context is about to complete, or on a length-1 bad
    word unconditionally. transformers 5.20.2 crashes on the length-1 path under
    float64 (Index put dtype mismatch), so the oracle reproduces the intended
    semantics here and cross-checks the length >= 2 path against the real
    processor below."""
    filtered = [bw for bw in bad_words_ids if not (len(bw) == 1 and bw[0] in eos_ids)]
    result = list(scores)
    for bw in filtered:
        if len(bw) == 1:
            result[bw[0]] = NEG_INF
        else:
            prefix = bw[:-1]
            if len(ids) >= len(prefix) and ids[-len(prefix):] == prefix:
                result[bw[-1]] = NEG_INF
    return result


def add_bad_words(C):
    bad_words = [[2], [1, 3], [5], [0, 2, 4]]
    ids_rows = [
        [1, 3],
        [9, 1],
        [9, 1, 3],
        [0, 2],
        [0, 2, 4],
        [5, 5],
    ]
    cases = []
    for ids in ids_rows:
        cases.append((bad_words, ids, R0, bad_words_ref(bad_words, [5], ids, R0)))
    # Cross-check the length >= 2 subset against the real processor (float64-safe).
    long_bad_words = [[1, 3], [0, 2, 4]]
    for ids in ids_rows:
        proc = NoBadWordsLogitsProcessor(bad_words_ids=long_bad_words, eos_token_id=[5])
        real = run1(proc, ids, R0)
        ref = bad_words_ref(long_bad_words, [5], ids, R0)
        assert real == ref, (ids, real, ref)
    C["BAD_BADWORDS"] = [b for b, _, _, _ in cases]
    C["BAD_IDS"] = [i for _, i, _, _ in cases]
    C["BAD_SCORES"] = [s for _, _, s, _ in cases]
    C["BAD_EXPECTED"] = [e for _, _, _, e in cases]


def add_composition(C):
    # Chain: temperature -> top_k -> top_p over a few rows.
    chain = [
        TemperatureLogitsWarper(temperature=2.0),
        TopKLogitsWarper(top_k=3),
        TopPLogitsWarper(top_p=0.9),
    ]
    rows = [R0, R2, R4, R6, R7]
    C["COMPOSE_IDS"] = [[] for _ in rows]
    C["COMPOSE_SCORES"] = rows
    C["COMPOSE_EXPECTED"] = [run1_list(LogitsProcessorList(chain), [], r) for r in rows]

    # Order sensitivity: top_k then temperature vs temperature then top_k.
    a = [TopKLogitsWarper(top_k=2), TemperatureLogitsWarper(temperature=2.0)]
    b = [TemperatureLogitsWarper(temperature=2.0), TopKLogitsWarper(top_k=2)]
    C["ORDER_SCORES"] = [R0, R6]
    C["ORDER_A_EXPECTED"] = [run1_list(LogitsProcessorList(a), [], r) for r in (R0, R6)]
    C["ORDER_B_EXPECTED"] = [run1_list(LogitsProcessorList(b), [], r) for r in (R0, R6)]


def add_stopping(C):
    # MaxLengthCriteria and EosTokenCriteria are tested directly in the port suite
    # against the real classes' verdicts on token rows.
    ids_rows = [
        [1, 2, 3],
        [1, 2, 3, 4],
        [1, 2, 3, 4, 5],
        [1, 2, 3, 4, 5, 6],
        [1, 2, 4],
        [1, 2, 3, 2, 4],
    ]
    C["STOP_IDS"] = ids_rows

    ml_cases = []
    for ml in (3, 4, 5, 6):
        crit = MaxLengthCriteria(max_length=ml)
        for ids in ids_rows:
            done = crit(torch.tensor([ids]), None)
            ml_cases.append(bool(done[0].item()))
    C["STOP_MAXLEN_MAX"] = [ml for ml in (3, 4, 5, 6) for _ in ids_rows]
    C["STOP_MAXLEN_DONE"] = ml_cases

    eos_cases = []
    for eos in ([4], [2, 4]):
        for ids in ids_rows:
            crit = EosTokenCriteria(eos_token_id=eos)
            done = crit(torch.tensor([ids]), None)
            eos_cases.append(bool(done[0].item()))
    C["STOP_EOS_EOS"] = [[4] for _ in ids_rows] + [[2, 4] for _ in ids_rows]
    C["STOP_EOS_DONE"] = eos_cases


def add_helpers(C):
    """`Lists` helpers against Python's own `in` and tail slicing, and the port's
    `stopped_any` against a StoppingCriteriaList over the stopping rows."""
    xs_rows = [[4, 7, 9], [4, 7, 9], [], [7], [1, 1, 2]]
    xs_x = [7, 5, 5, 7, 1]
    C["HELP_CONTAINS_XS"] = xs_rows
    C["HELP_CONTAINS_X"] = xs_x
    C["HELP_CONTAINS_OUT"] = [x in xs for xs, x in zip(xs_rows, xs_x)]
    tails = [([1, 2, 3, 4], 2), ([1, 2, 3], 5), ([1, 2, 3], 0), ([], 3), ([9], 1)]
    C["HELP_LASTN_XS"] = [xs for xs, _ in tails]
    C["HELP_LASTN_N"] = [n for _, n in tails]
    C["HELP_LASTN_OUT"] = [xs[len(xs) - min(n, len(xs)):] for xs, n in tails]
    crits = StoppingCriteriaList([MaxLengthCriteria(max_length=5), EosTokenCriteria(eos_token_id=[4])])
    C["STOPANY_DONE"] = [bool(crits(torch.tensor([ids]), None)[0].item()) for ids in C["STOP_IDS"]]


def add_generate(C):
    # A synthetic model: at each step it returns the next row of logits. The
    # driver must use the real processors and torch.argmax, not a Python loop
    # that re-derives them.
    step_logits = [
        [0.5, 1.0, 2.5, 0.2, 3.0],
        [0.1, 3.0, 0.9, 2.0, 0.4],
        [0.0, 0.2, 0.3, 0.1, 4.0],
        [2.0, 0.5, 0.5, 1.0, 0.1],
        [0.3, 0.3, 0.3, 0.3, 0.3],
    ]
    prompt = [1, 2]
    eos = [4]
    processors = LogitsProcessorList(
        [MinLengthLogitsProcessor(min_length=3, eos_token_id=eos), SuppressTokensLogitsProcessor([1])]
    )
    stopping = StoppingCriteriaList([MaxLengthCriteria(max_length=7), EosTokenCriteria(eos_token_id=eos)])

    def ref_generate(tokens):
        if len(tokens) >= 7:
            return tokens
        idx = len(tokens) - len(prompt)
        scores = processors(torch.tensor([tokens]), torch.tensor([step_logits[idx]]))[0]
        token = int(torch.argmax(scores).item())
        grown = tokens + [token]
        if bool(stopping(torch.tensor([grown]), None)[0].item()):
            return grown
        return ref_generate(grown)

    C["GEN_STEP_LOGITS"] = step_logits
    C["GEN_PROMPT"] = prompt
    C["GEN_EOS"] = eos
    C["GEN_EXPECTED"] = ref_generate(prompt)


def add_stop_string(C):
    # The port matches decoded-text suffix semantics (Python str.endswith).
    texts = [
        "The capital of France is Paris",
        "Hello world",
        "stop",
        "unfinished business",
        "a longer sentence ending with <|end|>",
        "",
        "exactlystop",
    ]
    stop_sets = [
        ["Paris", "London"],
        ["world", "earth"],
        ["stop"],
        ["business"],
        ["<|end|>"],
        [""],
        ["stop", "exactlystop"],
    ]
    C["STOPSTR_TEXTS"] = texts
    C["STOPSTR_STRS"] = stop_sets
    C["STOPSTR_DONE"] = [any(t.endswith(s) for s in ss) for t, ss in zip(texts, stop_sets)]


def main():
    C = {}
    add_math(C)
    add_temperature(C)
    add_top_k(C)
    add_top_p(C)
    add_min_p(C)
    add_typical(C)
    add_epsilon(C)
    add_eta(C)
    add_repetition_penalty(C)
    add_no_repeat_ngram(C)
    add_min_length(C)
    add_min_new_tokens(C)
    add_forced_bos(C)
    add_forced_eos(C)
    add_suppress_tokens(C)
    add_bad_words(C)
    add_composition(C)
    add_stopping(C)
    add_helpers(C)
    add_stop_string(C)
    add_generate(C)

    target = write(
        "transformers",
        "transformers generation layer: logits warpers/processors, torch softmax/log_softmax/argmax/inverse-CDF multinomial, and stopping criteria (float64).",
        {"transformers": transformers.__version__, "torch": torch.__version__},
        C,
    )
    print(f"wrote {target} with {len(C)} exports")


if __name__ == "__main__":
    main()
