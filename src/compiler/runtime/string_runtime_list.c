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
    osp_string_list *list = (osp_string_list *)malloc(sizeof(osp_string_list));
    if (!list) return NULL;
    /* `{ i64 length; char **items }` is exactly the LIST_HDR_PTR shape, so one
     * release reclaims the whole list: items[0..length), then the array. Only
     * the live prefix is walked, which is why the (malloc'd, uninitialised)
     * capacity slack past `length` is harmless. Codegen owns these handles and
     * drops them at region end — osp_string_list_free is the C-test path only.
     * No-op off ARC. [GC-ARC-PERCEUS] [MEM-BACKENDS-ELEMENTS]. */
    osp_mem_set_layout(list, OSP_MEM_LIST_HDR_PTR);
    list->length = 0;
    if (capacity <= 0) {
        list->items = NULL;
        return list;
    }
    list->items = (char **)malloc((size_t)capacity * sizeof(char *));
    if (!list->items) {
        free(list);
        return NULL;
    }
    return list;
}

osp_string_list *osp_string_split(const char *s, const char *sep) {
    if (!s || !sep || sep[0] == '\0') return NULL;
    size_t seplen = strlen(sep);

    int64_t cap = 1;
    for (const char *p = s; (p = strstr(p, sep)) != NULL; p += seplen) cap++;

    osp_string_list *list = osp_list_new(cap);
    if (!list) return NULL;

    const char *r = s;
    while (1) {
        const char *hit = strstr(r, sep);
        if (!hit) {
            list->items[list->length++] = osp_string_dup_internal(r, strlen(r));
            break;
        }
        list->items[list->length++] = osp_string_dup_internal(r, (size_t)(hit - r));
        r = hit + seplen;
    }
    return list;
}

osp_string_list *osp_string_lines(const char *s) {
    if (!s) return osp_list_new(0);
    size_t len = strlen(s);

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
            list->items[list->length++] =
                osp_string_dup_internal(start, (size_t)(s + i - start));
            start = s + i + 1;
        }
    }
    if (start < s + len)
        list->items[list->length++] =
            osp_string_dup_internal(start, (size_t)(s + len - start));
    return list;
}

osp_string_list *osp_string_words(const char *s) {
    if (!s) return osp_list_new(0);
    size_t len = strlen(s);

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
        list->items[list->length++] = osp_string_dup_internal(s + start, i - start);
    }
    return list;
}

char *osp_string_join(const osp_string_list *list, const char *sep) {
    if (!list || !sep) return osp_string_empty_internal();
    if (list->length == 0) return osp_string_empty_internal();
    size_t seplen = strlen(sep);
    size_t total = 0;
    for (int64_t i = 0; i < list->length; i++)
        if (list->items[i]) total += strlen(list->items[i]);
    total += seplen * (size_t)(list->length - 1);
    char *out = (char *)malloc(total + 1);
    if (!out) return NULL;
    char *w = out;
    for (int64_t i = 0; i < list->length; i++) {
        if (i > 0) {
            memcpy(w, sep, seplen);
            w += seplen;
        }
        if (list->items[i]) {
            size_t l = strlen(list->items[i]);
            memcpy(w, list->items[i], l);
            w += l;
        }
    }
    *w = '\0';
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
        view.items = (char **)malloc(sizeof(char *) * (size_t)n);
        if (!view.items) return NULL;
        for (int64_t i = 0; i < n; i++)
            view.items[i] = (char *)(intptr_t)osprey_list_get(list, i);
    }
    char *out = osp_string_join(&view, sep);
    free(view.items);
    return out;
}

void osp_string_list_free(osp_string_list *list) {
    if (!list) return;
    if (list->items) {
        for (int64_t i = 0; i < list->length; i++) free(list->items[i]);
        free(list->items);
    }
    free(list);
}
