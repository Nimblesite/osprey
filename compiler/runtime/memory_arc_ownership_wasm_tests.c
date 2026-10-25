#include "memory_hooks.h"
#include <stdint.h>
#include <stdio.h>
#include <string.h>
// A scalar that equals a live object's address still owns no reference.
// Run on wasm32; the two fields share one 8-byte layout-mask word.
static int32_t count(void *p) { int32_t n; memcpy(&n, (char *)p - 8, 4); return n; }
static int scalar_ownership(void) {
    void *child = osp_alloc_tagged(8, OSP_MEM_RAW);
    void *other = osp_alloc_tagged(8, OSP_MEM_RAW);
    void *parent = osp_alloc_tagged(8, OSP_MEM_LAYOUT(OSP_MEM_WORD(0)));
    if (!child || !other || !parent || sizeof(void *) != 4) return 2;
    osp_retain(child);
    osp_retain(other);
    uint32_t scalar = (uint32_t)(uintptr_t)other;
    memcpy(parent, &child, 4);
    memcpy((char *)parent + 4, &scalar, 4);
    osp_release(parent);
    int32_t actual = count(other);
    printf("unrelated scalar target reference count: expected=2 actual=%d\n", actual);
    osp_release(child);
    osp_release(other);
    osp_release(other);
    return actual == 2 ? 0 : 1;
}

static int pointer_list_ownership(void) {
    void *first = osp_alloc_tagged(8, OSP_MEM_RAW);
    void *second = osp_alloc_tagged(8, OSP_MEM_RAW);
    void *other = osp_alloc_tagged(8, OSP_MEM_RAW);
    void **data = osp_alloc_tagged(4 * sizeof(void *), OSP_MEM_RAW);
    struct { int64_t length; void **data; } *list = osp_alloc_tagged(16, OSP_MEM_LIST_HDR_PTR);
    if (!first || !second || !other || !data || !list) return 2;
    osp_retain(first); osp_retain(second); osp_retain(other);
    data[0] = first; data[1] = second; data[2] = other; data[3] = NULL;
    list->length = 2; list->data = data;
    osp_release(list);
    int valid = count(first) == 1 && count(second) == 1 && count(other) == 2;
    printf("pointer list children: expected=1,1,2 actual=%d,%d,%d\n", count(first), count(second), count(other));
    osp_release(first); osp_release(second); osp_release(other); osp_release(other);
    return valid ? 0 : 1;
}

int main(void) {
    int scalar = scalar_ownership();
    int pointers = pointer_list_ownership();
    return scalar != 0 || pointers != 0;
}
