# osprey: flavor=python
# Implements [PYTHON-DATASET-AUDIT].
# Native training-data acceptance policy. No dataset or Python I/O runs here.

def privacy_issue(secret, personal, allow_personal):
    return 2 if secret else 3 if personal and not allow_personal else 0


def overlap_issue(is_train, evaluation_match, evaluation_similarity, duplicate, conflict, local_similarity, threshold):
    return (4 if is_train and evaluation_match else
            7 if is_train and threshold > 0.0 and evaluation_similarity >= threshold else
            5 if conflict else 6 if duplicate else
            8 if threshold > 0.0 and local_similarity >= threshold else 0)


def length_issue(chars, max_chars, tokens, max_tokens, completion_chars: int, minimum_completion):
    return (9 if (max_chars > 0 and chars > max_chars) or (max_tokens > 0 and tokens > max_tokens) else
            10 if completion_chars < minimum_completion else 0)


def decide(valid, secret, personal, allow_personal, is_train, evaluation_match, evaluation_similarity,
           duplicate, conflict, local_similarity, threshold, chars, max_chars, tokens, max_tokens,
           completion_chars, minimum_completion):
    privacy = privacy_issue(secret, personal, allow_personal)
    overlap = overlap_issue(is_train, evaluation_match, evaluation_similarity, duplicate, conflict, local_similarity, threshold)
    limits = length_issue(chars, max_chars, tokens, max_tokens, completion_chars, minimum_completion)
    return 1 if not valid else privacy if privacy != 0 else overlap if overlap != 0 else limits


def reason_name(code):
    return ("keep" if code == 0 else
            "invalid_record" if code == 1 else "exposed_secret" if code == 2 else
            "personal_data" if code == 3 else "evaluation_leakage" if code == 4 else
            "conflicting_target" if code == 5 else "duplicate" if code == 6 else
            "near_evaluation_leakage" if code == 7 else "near_duplicate" if code == 8 else
            "context_limit" if code == 9 else "empty_completion" if code == 10 else "invalid_decision")


def valid_decision(code):
    return code >= 0 and code <= 10


def policy_version():
    return 1


@extern
def osprey_python_object(operation: str, object_id: int, name: str, args: str) -> str:
    ...


@effect
class Audit:
    def inspect(session: int, row: int) -> None: ...


@handler(Audit)
class PythonServices:
    def inspect(session, row):
        # The worker verifies every row completed before committing; a failed
        # callback cannot become a successful, partially audited output.
        _ = osprey_python_object("call_value", session, "inspect", '{"args":[{"value":' + toString(row) + '}]}')
        return None


def audit_batch(session, start, count):
    with PythonServices:
        return forEach(range(count), lambda offset: perform(Audit.inspect(session, satAdd(start, offset))))
