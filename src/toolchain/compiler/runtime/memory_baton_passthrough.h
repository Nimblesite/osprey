// The baton and scanned-block hooks (memory_hooks.h) for a backend with no
// tracer to inform: scanned blocks are plain malloc and parking is free.
#ifndef OSPREY_MEMORY_BATON_PASSTHROUGH_H
#define OSPREY_MEMORY_BATON_PASSTHROUGH_H
#include <stdlib.h>

void *osp_mem_scanned_alloc(size_t size) { return malloc(size); }
void *osp_mem_scanned_realloc(void *block, size_t size) { return realloc(block, size); }
void osp_mem_scanned_free(void *block) { free(block); }
void osp_mem_baton_prepare(void) {}
void osp_mem_baton_thread_enter(void) {}
void osp_mem_baton_thread_exit(void) {}
void osp_mem_baton_park(const void *lowest) { (void)lowest; }
void osp_mem_baton_unpark(void) {}
#endif
