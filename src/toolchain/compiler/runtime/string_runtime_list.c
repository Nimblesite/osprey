/*
 * Implements [BUILTIN-STRING-LIST]: [BUILTIN-STRING-SPLIT],
 * [BUILTIN-STRING-LINES], [BUILTIN-STRING-WORDS], and [BUILTIN-STRING-JOIN].
 * List-returning string helpers (split, lines, words, join).
 * Scalars live in string_runtime.c.
 */

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "collection_runtime.h"
#include "memory_hooks.h"
#include "string_runtime.h"

static osp_string_list *osp_list_new(int64_t capacity) {
    if (capacity < 0 || (uint64_t)capacity > UINT32_MAX / sizeof(char *)) return NULL;
    osp_string_list *list = osp_alloc_tagged(sizeof(osp_string_list), OSP_MEM_LIST_HDR_PTR);
    if (!list) return NULL;
    list->length = 0; list->items = NULL;
    if (capacity == 0) return list;
    list->items = osp_alloc_tagged((int64_t)((size_t)capacity * sizeof(char *)), OSP_MEM_RAW);
    if (!list->items) { osp_release(list); return NULL; }
    return list;
}

static int osp_list_piece(osp_string_list *list, const char *bytes, size_t n) {
    char *text = osp_string_dup_internal(bytes, n);
    if (!text) { osp_release(list); return 0; }
    list->items[list->length++] = text;
    return 1;
}

osp_string_list *osp_string_split(const char *s, const char *sep) {
    if (!s || !sep || osp_strlen(sep) == 0) return NULL;
    size_t length = (size_t)osp_strlen(s), width = (size_t)osp_strlen(sep), offset = 0;
    int64_t capacity = (int64_t)osp_string_count_internal(s, length, sep, width) + 1;
    const char *hit;
    osp_string_list *list = osp_list_new(capacity);
    if (!list) return NULL;
    offset = 0;
    while ((hit = osp_string_find_internal(s + offset, length - offset, sep, width)) != NULL) {
        if (!osp_list_piece(list, s + offset, (size_t)(hit - s) - offset)) return NULL;
        offset = (size_t)(hit - s) + width;
    }
    return osp_list_piece(list, s + offset, length - offset) ? list : NULL;
}

osp_string_list *osp_string_lines(const char *s) {
    if (!s) return osp_list_new(0);
    size_t len = (size_t)osp_strlen(s);

    int64_t cap = 0;
    for (size_t i = 0; i < len; i++)
        if (s[i] == '\n') cap++;
    if (len > 0 && s[len - 1] != '\n') cap++;
    if (cap == 0) return osp_list_new(0);

    osp_string_list *list = osp_list_new(cap);
    if (!list) return NULL;
    const char *start = s;
    for (size_t i = 0; i < len; i++) {
        if (s[i] == '\n') {
            if (!osp_list_piece(list, start, (size_t)(s + i - start))) return NULL;
            start = s + i + 1;
        }
    }
    if (start < s + len && !osp_list_piece(list, start, (size_t)(s + len - start))) return NULL;
    return list;
}

osp_string_list *osp_string_words(const char *s) {
    if (!s) return osp_list_new(0);
    size_t len = (size_t)osp_strlen(s);

    int64_t cap = 0;
    int in_word = 0;
    for (size_t i = 0; i < len; i++) {
        if (osp_is_ws_internal((unsigned char)s[i])) {
            in_word = 0;
        } else if (!in_word) {
            in_word = 1;
            cap++;
        }
    }
    if (cap == 0) return osp_list_new(0);

    osp_string_list *list = osp_list_new(cap);
    if (!list) return NULL;
    size_t i = 0;
    while (i < len) {
        while (i < len && osp_is_ws_internal((unsigned char)s[i])) i++;
        if (i >= len) break;
        size_t start = i;
        while (i < len && !osp_is_ws_internal((unsigned char)s[i])) i++;
        if (!osp_list_piece(list, s + start, i - start)) return NULL;
    }
    return list;
}

char *osp_string_join(const osp_string_list *list, const char *sep) {
    if (!list || !sep || list->length <= 0) return osp_string_empty_internal();
    size_t width = (size_t)osp_strlen(sep), total = 0;
    for (int64_t i = 0; i < list->length; i++) {
        size_t n = list->items[i] ? (size_t)osp_strlen(list->items[i]) : 0;
        if (n > UINT32_MAX - 1 - total) return NULL;
        total += n;
        if (i > 0) { if (width > UINT32_MAX - 1 - total) return NULL; total += width; }
    }
    char *out = osp_string_alloc_internal(total), *w = out;
    if (!out) return NULL;
    for (int64_t i = 0; i < list->length; i++) {
        if (i > 0) { memcpy(w, sep, width); w += width; }
        size_t n = list->items[i] ? (size_t)osp_strlen(list->items[i]) : 0;
        if (n) { memcpy(w, list->items[i], n); w += n; }
    }
    return out;
}

// `join` over a RUNTIME list — the layout every list builtin produces
// (listAppend, toList, a fold) and every `let`-bound string list settles to.
// The elements are the `char *` words the list stores; they are borrowed into
// a flat view for osp_string_join, so the two layouts share one join.
char *osp_string_join_list(OspreyList *list, const char *sep) {
    int64_t n = list ? osprey_list_length(list) : 0;
    osp_string_list view = {n, NULL};
    if (n > 0) {
        if ((uint64_t)n > SIZE_MAX / sizeof(char *)) return NULL;
        view.items = (char **)malloc(sizeof(char *) * (size_t)n);
        if (!view.items) return NULL;
        for (int64_t i = 0; i < n; i++)
            view.items[i] = (char *)(intptr_t)osprey_list_get(list, i);
    }
    char *out = osp_string_join(&view, sep);
    free(view.items);
    return out;
}

void osp_string_list_free(osp_string_list *list) { osp_release(list); }
