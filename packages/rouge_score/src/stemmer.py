# osprey: flavor=python
# osprey: namespace=rouge_score
# osprey: module=Stemmer
"""The Porter stemming algorithm as nltk's `PorterStemmer` implements it in
`NLTK_EXTENSIONS` mode (the default), which is what rouge_score's
`DefaultTokenizer(use_stemmer=True)` uses. Only the lowercasing `stem(word)`
form is ported.
"""

__all__ = ["stem"]


def char_at(word, i):
    """The one-character string at (ASCII) index `i` of `word`."""
    return unwrap_or(substring(word, i, i + 1), "")


def is_vowel_char(c):
    """Whether the one-character string `c` is a vowel."""
    return c == "a" or c == "e" or c == "i" or c == "o" or c == "u"


def y_consonant(word, i, negate):
    """Resolves a run of `y` characters backwards to decide consonant-ness."""
    if i > 0 and char_at(word, i) == "y":
        return y_consonant(word, i - 1, not negate)
    else:
        return (not is_vowel_char(char_at(word, i))) != negate


def is_consonant(word, i):
    """Porter's consonant test for `word[i]`."""
    c = char_at(word, i)
    if is_vowel_char(c):
        return False
    elif c == "y":
        return y_consonant(word, i, False)
    else:
        return True


def measure_from(stem, i, acc):
    """Counts vowel-consonant transitions, the Porter measure."""
    if i >= length(stem):
        return acc
    else:
        is_vc = (not is_consonant(stem, i - 1)) and is_consonant(stem, i)
        return measure_from(stem, i + 1, acc + 1 if is_vc else acc)


def measure(stem):
    """The Porter measure of `stem`."""
    return measure_from(stem, 1, 0)


def contains_vowel_from(stem, i):
    """Whether `stem[i:]` contains a vowel."""
    if i >= length(stem):
        return False
    elif not is_consonant(stem, i):
        return True
    else:
        return contains_vowel_from(stem, i + 1)


def contains_vowel(stem):
    """Whether `stem` contains a vowel."""
    return contains_vowel_from(stem, 0)


def ends_double_consonant(word):
    """Porter's `*d`: the word ends in a double consonant."""
    if length(word) >= 2:
        last = char_at(word, length(word) - 1)
        return last == char_at(word, length(word) - 2) and is_consonant(word, length(word) - 1)
    else:
        return False


def in_wxy(c):
    """Whether `c` is `w`, `x` or `y`."""
    return c == "w" or c == "x" or c == "y"


def ends_cvc(word):
    """Porter's `*o`: the stem ends consonant-vowel-consonant."""
    n = length(word)
    if n >= 3:
        return is_consonant(word, n - 3) and not is_consonant(word, n - 2) and is_consonant(word, n - 1) and not in_wxy(char_at(word, n - 1))
    elif n == 2:
        return not is_consonant(word, 0) and is_consonant(word, 1)
    else:
        return False


def replace_suffix(word, suffix, replacement):
    """`word` with a matching `suffix` swapped for `replacement`."""
    if suffix == "":
        return word + replacement
    else:
        return take(word, length(word) - length(suffix)) + replacement


def step1a(word):
    """Step 1a: plural-suffix removal."""
    if endsWith(word, "ies") and length(word) == 4:
        return replace_suffix(word, "ies", "ie")
    elif endsWith(word, "sses"):
        return replace_suffix(word, "sses", "ss")
    elif endsWith(word, "ies"):
        return replace_suffix(word, "ies", "i")
    elif endsWith(word, "ss"):
        return replace_suffix(word, "ss", "ss")
    elif endsWith(word, "s"):
        return replace_suffix(word, "s", "")
    else:
        return word


def in_lsz(c):
    """Whether `c` is `l`, `s` or `z`."""
    return c == "l" or c == "s" or c == "z"


def step1b_rules(stem):
    """Step 1b's follow-up rules applied to the intermediate stem."""
    if endsWith(stem, "at"):
        return replace_suffix(stem, "at", "ate")
    elif endsWith(stem, "bl"):
        return replace_suffix(stem, "bl", "ble")
    elif endsWith(stem, "iz"):
        return replace_suffix(stem, "iz", "ize")
    elif ends_double_consonant(stem):
        if in_lsz(char_at(stem, length(stem) - 1)):
            return stem
        else:
            return take(stem, length(stem) - 1)
    elif measure(stem) == 1 and ends_cvc(stem):
        return stem + "e"
    else:
        return stem


def step1b(word):
    """Step 1b: `eed`, `ed` and `ing` removal."""
    if endsWith(word, "ied"):
        return replace_suffix(word, "ied", "ie") if length(word) == 4 else replace_suffix(word, "ied", "i")
    elif endsWith(word, "eed"):
        stem = replace_suffix(word, "eed", "")
        return stem + "ee" if measure(stem) > 0 else word
    elif endsWith(word, "ed"):
        stem = replace_suffix(word, "ed", "")
        return step1b_rules(stem) if contains_vowel(stem) else word
    elif endsWith(word, "ing"):
        stem = replace_suffix(word, "ing", "")
        return step1b_rules(stem) if contains_vowel(stem) else word
    else:
        return word


def step1c(word):
    """Step 1c: terminal `y` becomes `i` after a consonant."""
    if endsWith(word, "y"):
        stem = take(word, length(word) - 1)
        if length(stem) > 1 and is_consonant(stem, length(stem) - 1):
            return stem + "i"
        else:
            return word
    else:
        return word


STEP2_SUFFIXES = ["ational", "tional", "enci", "anci", "izer", "bli", "alli", "entli", "eli", "ousli", "ization", "ation", "ator", "alism", "iveness", "fulness", "ousness", "aliti", "iviti", "biliti", "fulli", "logi"]
STEP2_REPLACEMENTS = ["ate", "tion", "ence", "ance", "ize", "ble", "al", "ent", "e", "ous", "ize", "ate", "ate", "al", "ive", "ful", "ous", "al", "ive", "ble", "ful", "log"]


def step2_condition(word, suffix):
    """Step 2's measure condition (`logi` measures without its last three)."""
    if suffix == "logi":
        return measure(take(word, length(word) - 3)) > 0
    else:
        return measure(take(word, length(word) - length(suffix))) > 0


def step2_rules(word, i):
    """Step 2's suffix table, tried in order."""
    if i >= len(STEP2_SUFFIXES):
        return word
    else:
        suffix = unwrap_or(listGet(STEP2_SUFFIXES, i), "")
        if endsWith(word, suffix):
            stem = take(word, length(word) - length(suffix))
            return stem + unwrap_or(listGet(STEP2_REPLACEMENTS, i), "") if step2_condition(word, suffix) else word
        else:
            return step2_rules(word, i + 1)


def step2(word):
    """Step 2: derivational suffix removal."""
    if endsWith(word, "alli") and measure(take(word, length(word) - 4)) > 0:
        return step2(replace_suffix(word, "alli", "al"))
    else:
        return step2_rules(word, 0)


STEP3_SUFFIXES = ["icate", "ative", "alize", "iciti", "ical", "ful", "ness"]
STEP3_REPLACEMENTS = ["ic", "", "al", "ic", "ic", "", ""]


def step3_rules(word, i):
    """Step 3's suffix table, tried in order."""
    if i >= len(STEP3_SUFFIXES):
        return word
    else:
        suffix = unwrap_or(listGet(STEP3_SUFFIXES, i), "")
        if endsWith(word, suffix):
            stem = take(word, length(word) - length(suffix))
            return stem + unwrap_or(listGet(STEP3_REPLACEMENTS, i), "") if measure(stem) > 0 else word
        else:
            return step3_rules(word, i + 1)


def step3(word):
    """Step 3: further derivational suffix removal."""
    return step3_rules(word, 0)


STEP4_SUFFIXES = ["al", "ance", "ence", "er", "ic", "able", "ible", "ant", "ement", "ment", "ent", "ion", "ou", "ism", "ate", "iti", "ous", "ive", "ize"]


def in_st(c):
    """Whether `c` is `s` or `t`."""
    return c == "s" or c == "t"


def step4_condition(stem, suffix):
    """Step 4's condition: measure > 1 (and `ion` needs a `s`/`t` stem end)."""
    if suffix == "ion":
        return measure(stem) > 1 and in_st(char_at(stem, length(stem) - 1))
    else:
        return measure(stem) > 1


def step4_rules(word, i):
    """Step 4's suffix table, tried in order."""
    if i >= len(STEP4_SUFFIXES):
        return word
    else:
        suffix = unwrap_or(listGet(STEP4_SUFFIXES, i), "")
        if endsWith(word, suffix):
            stem = take(word, length(word) - length(suffix))
            return stem if step4_condition(stem, suffix) else word
        else:
            return step4_rules(word, i + 1)


def step4(word):
    """Step 4: long-suffix removal under measure > 1."""
    return step4_rules(word, 0)


def step5a(word):
    """Step 5a: final `e` removal."""
    if endsWith(word, "e"):
        stem = take(word, length(word) - 1)
        if measure(stem) > 1:
            return stem
        elif measure(stem) == 1 and not ends_cvc(stem):
            return stem
        else:
            return word
    else:
        return word


def step5b(word):
    """Step 5b: final double `l` becomes single."""
    if endsWith(word, "ll"):
        return take(word, length(word) - 1) if measure(take(word, length(word) - 1)) > 1 else word
    else:
        return word


POOL = {"sky": "sky", "skies": "sky", "dying": "die", "lying": "lie", "tying": "tie",
        "news": "news", "innings": "inning", "inning": "inning", "outings": "outing",
        "outing": "outing", "cannings": "canning", "canning": "canning", "howe": "howe",
        "proceed": "proceed", "exceed": "exceed", "succeed": "succeed"}


def stem(word):
    """`PorterStemmer.stem(word)` in NLTK_EXTENSIONS mode."""
    s = toLowerCase(word)
    if mapContains(POOL, s):
        return unwrap_or(mapGet(POOL, s), s)
    elif length(word) <= 2:
        return s
    else:
        return step5b(step5a(step4(step3(step2(step1c(step1b(step1a(s))))))))
