// `loadTensor` [BUILTIN-FILE-TENSOR]: one safetensors tensor read by the
// shared reader (tensor_file.h) straight into a host List<float>. No GpuBuffer
// and no osprey_gpu_* symbol exists on this path, so a program that loads its
// weights here links nothing of the GPU surface.

// fseeko: file offsets past 2 GiB on 32-bit targets, hidden under strict C11.
#define _POSIX_C_SOURCE 200809L

#include "collection_runtime.h"
#include "io_error.h"
#include "memory_hooks.h"
#include "tensor_file.h"

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char OP_LOAD_TENSOR[] = "loadTensor";
static const char OP_READ_BYTES[] = "readFileBytes";

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

// [BUILTIN-FILE-BYTES] At most `count` bytes of `path` from byte `offset` as a
// List<int> of 0..=255, read as binary so a NUL is one more byte. The list is
// shorter than `count` only because the file ended; a read error is NULL with
// the reason, never a shorter list.
OspreyList *osprey_read_file_bytes(const char *path, int64_t offset, int64_t count) {
  const char *refusal = offset < 0 ? "a negative offset" : count < 0 ? "a negative byte count" : NULL;
  if (refusal != NULL || path == NULL) {
    osp_io_error_text(OP_READ_BYTES, path, refusal ? refusal : "no path");
    return NULL;
  }
  FILE *file = fopen(path, "rb");
  if (file == NULL) {
    osp_io_error_set(OP_READ_BYTES, path, errno);
    return NULL;
  }
  OspreyListBuilder *builder = osprey_list_builder_new_of(0);
  unsigned char chunk[65536];
  int failure = fseeko(file, (off_t)offset, SEEK_SET) != 0 ? errno : 0;
  while (failure == 0 && count > 0) {
    size_t got = fread(chunk, 1, count < (int64_t)sizeof chunk ? (size_t)count : sizeof chunk, file);
    for (size_t i = 0; i < got; i++) {
      osprey_list_builder_push_of(builder, chunk[i], 0);
    }
    count -= (int64_t)got;
    if (got == 0) {
      failure = ferror(file) ? errno : 0;
      break;
    }
  }
  (void)fclose(file);
  OspreyList *list = osprey_list_builder_seal(builder);
  if (failure != 0) {
    osp_release(list);
    osp_io_error_set(OP_READ_BYTES, path, failure);
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
