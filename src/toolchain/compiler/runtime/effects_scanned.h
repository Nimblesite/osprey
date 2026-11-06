// Runtime-owned effect state holds managed pointers no value on any stack names:
// operands in a mailbox, resume values, handler environments. These blocks must
// be visible to the collector for as long as they live [MEM-EFFECT-BATON].
// Include after <stdlib.h>.
#ifndef OSPREY_EFFECTS_SCANNED_H
#define OSPREY_EFFECTS_SCANNED_H
#include "memory_hooks.h"
#define malloc(size) osp_mem_scanned_alloc(size)
#define realloc(block, size) osp_mem_scanned_realloc((block), (size))
#define free(block) osp_mem_scanned_free(block)
#endif
