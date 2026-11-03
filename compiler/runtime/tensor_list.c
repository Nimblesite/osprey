// `loadTensor` [BUILTIN-FILE-TENSOR]: one safetensors tensor read by the
// shared reader (tensor_file.h) straight into a host List<float>. No GpuBuffer
// and no osprey_gpu_* symbol exists on this path, so a program that loads its
// weights here links nothing of the GPU surface.

#include "collection_runtime.h"
#include "memory_hooks.h"
#include "tensor_file.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char OP_LOAD_TENSOR[] = "loadTensor";

// The reader's sink context: the span names the stored format.
typedef struct {
  OspreyListBuilder *builder;
  OspTensorSpan span;
} ListLoad;

// Push each element's widened value as the IEEE-754 word a List<float>
// element holds: the reader hands the STORED bytes [GPU-BUFFER-ELEM], and the
// one widening definition is osp_tensor_widen's.
static void into_list(void *ctx, const unsigned char *bytes, size_t n) {
  const OspTensorSpan *span = &((ListLoad *)ctx)->span;
  for (size_t i = 0; i < n; i++) {
    double value = osp_tensor_widen(span->dtype, bytes + i * osp_tensor_width(span->dtype));
    int64_t word;
    memcpy(&word, &value, sizeof word);
    osprey_list_builder_push_of(((ListLoad *)ctx)->builder, word, 0);
  }
}

// One tensor out of a safetensors file as a List<float>, or NULL with the
// reason recorded. A list built before a failed read is released, so a NULL
// result owes the memory backend nothing.
OspreyList *osprey_load_tensor(const char *path, const char *name) {
  OspTensorSpan span;
  if (!osp_tensor_locate(OP_LOAD_TENSOR, path, name, &span)) {
    return NULL;
  }
  ListLoad load = {osprey_list_builder_new_of(0), span};
  int read = osp_tensor_read(OP_LOAD_TENSOR, path, &span, into_list, &load);
  OspreyList *list = osprey_list_builder_seal(load.builder);
  if (!read) {
    osp_release(list);
    return NULL;
  }
  return list;
}

// Broken-code quarantine: a host interpretation whose weights fail to load
// would otherwise run on whatever its caller substituted. It aborts naming why.
void *osprey_tensor_panic(const char *message) {
  (void)fprintf(stderr, "tensor load cannot continue: %s\n", message ? message : "unknown failure");
  abort();
}
