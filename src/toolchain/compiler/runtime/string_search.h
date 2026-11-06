// [PORT-STRING-NUL-OPERATIONS] Original bounded-byte implementation of the
// Crochemore–Perrin Two-Way algorithm (1991): constant scratch space, no allocation.
// Algorithm reference: https://monge.univ-mlv.fr/~mac/Articles-PDF/CP-1991-jacm.pdf
#ifndef OSP_STRING_SEARCH_H
#define OSP_STRING_SEARCH_H

// `boundary` is one past the maximal suffix start; the empty suffix uses zero.
static size_t string_critical_suffix(const unsigned char *text, size_t length,
                                     int reversed, size_t *period) {
    size_t boundary = 0, candidate = 0, matched = 1;
    *period = 1;
    while (candidate + matched < length) {
        unsigned char a = text[candidate + matched], b = text[boundary + matched - 1];
        if (a == b) {
            if (matched == *period) { candidate += *period; matched = 1; }
            else matched++;
        } else if ((a < b) != reversed) {
            candidate += matched; matched = 1; *period = candidate - boundary + 1;
        } else { boundary = ++candidate; matched = *period = 1; }
    }
    return boundary;
}

static const char *string_search_scan(const char *text, size_t length, const char *pattern,
                                     size_t width, size_t cut, size_t period, size_t retained) {
    size_t position = 0, known = 0;
    while (position <= length - width) {
        size_t at = cut > known ? cut : known;
        while (at < width && pattern[at] == text[position + at]) at++;
        if (at < width) { position += at - cut + 1; known = 0; continue; }
        at = cut;
        while (at > known && pattern[at - 1] == text[position + at - 1]) at--;
        if (at <= known) return text + position;
        position += period; known = retained;
    }
    return NULL;
}

const char *osp_string_find_internal(const char *text, size_t length,
                                      const char *pattern, size_t width) {
    if (width == 0) return text;
    if (width > length) return NULL;
    size_t forward_period, reverse_period;
    size_t left = string_critical_suffix((const unsigned char *)pattern, width, 0, &forward_period);
    size_t right = string_critical_suffix((const unsigned char *)pattern, width, 1, &reverse_period);
    size_t cut = left > right ? left : right;
    size_t period = left > right ? forward_period : reverse_period;
    size_t retained = width - period;
    if (period > width - cut || memcmp(pattern, pattern + period, cut) != 0) {
        period = (cut > width - cut ? cut : width - cut) + 1; retained = 0;
    }
    return string_search_scan(text, length, pattern, width, cut, period, retained);
}

size_t osp_string_count_internal(const char *text, size_t length, const char *pattern, size_t width) {
    if (!width) return 0;
    size_t offset = 0, count = 0;
    const char *hit;
    while ((hit = osp_string_find_internal(text + offset, length - offset, pattern, width)) != NULL) {
        count++; offset = (size_t)(hit - text) + width;
    }
    return count;
}

#endif
