// [NATIVE-SHM-SCENE] [NATIVE-SHM-VIEW] The pending runs of the one application
// executor: streamed shapes, native controls and their text. Each value is
// written once, here, into the storage a host later reads in place: no lock,
// no host call and no encoding per value. Portable C only, so the wasm runtime
// carries it too.
#include "native_bridge.h"
#include <stdlib.h>
#include <string.h>

enum { RUN_FIRST_BYTES = 4096 };
typedef struct {
  unsigned char *data;
  size_t length, capacity, limit;
  bool refused;
} NativeRun;
static NativeRun runs[OSP_NATIVE_RUNS] = {
    [OSP_NATIVE_SHAPES] = {.limit = OSP_NATIVE_SCENE_ROWS *
                                    sizeof(OspNativeShape)},
    [OSP_NATIVE_VIEWS] = {.limit = OSP_NATIVE_VIEW_LIMIT * sizeof(OspNativeView)},
    [OSP_NATIVE_TEXT] = {.limit = OSP_NATIVE_TEXT_LIMIT},
};

static bool run_grow(NativeRun *run, size_t wanted) {
  size_t next = run->capacity ? run->capacity : RUN_FIRST_BYTES;
  while (next < wanted)
    next *= 2;
  unsigned char *grown = realloc(run->data, next < run->limit ? next : run->limit);
  if (!grown)
    return false;
  run->data = grown;
  run->capacity = next < run->limit ? next : run->limit;
  return true;
}

// Room for `bytes` more at the end of `run`, or NULL once it is refused.
static void *run_reserve(NativeRun *run, size_t bytes) {
  if (run->refused || bytes > run->limit - run->length ||
      ((!run->data || run->length + bytes > run->capacity) &&
       !run_grow(run, run->length + bytes))) {
    run->refused = true;
    return NULL;
  }
  void *room = run->data + run->length;
  run->length += bytes;
  return room;
}

int64_t osp_native_scene_shape(int64_t code, int64_t index, double x, double y,
                               double width, double height, double angle,
                               int64_t colour, int64_t depth) {
  OspNativeShape *row = run_reserve(&runs[OSP_NATIVE_SHAPES], sizeof *row);
  if (!row)
    return 1;
  *row = (OspNativeShape){
      code, index, {x, y, width, height, angle}, colour, depth};
  return 0;
}

int64_t osp_native_view_row(int64_t kind, int64_t parent, int64_t action,
                            const char *text, int64_t colour,
                            int64_t background, double size, double padding,
                            double spacing, double radius, int64_t weight) {
  NativeRun *views = &runs[OSP_NATIVE_VIEWS], *chars = &runs[OSP_NATIVE_TEXT];
  int64_t index = (int64_t)(views->length / sizeof(OspNativeView));
  size_t bytes = text ? strlen(text) : 0, offset = chars->length;
  char *copy = parent < -1 || parent >= index ? NULL : run_reserve(chars, bytes);
  OspNativeView *row = copy ? run_reserve(views, sizeof *row) : NULL;
  if (!row) {
    views->refused = true;
    return -1;
  }
  if (bytes)
    memcpy(copy, text, bytes);
  *row = (OspNativeView){kind,    parent,  action,  (int64_t)offset,
                         (int64_t)bytes, colour, background, size,
                         padding, spacing, radius,  weight};
  return index;
}

void *osp_native_run_take(int64_t which, int64_t *length, int64_t *bytes,
                          bool *complete) {
  NativeRun *run = &runs[which], taken = *run;
  *length = (int64_t)taken.length;
  *bytes = (int64_t)taken.capacity;
  *complete = !taken.refused;
  *run = (NativeRun){.limit = taken.limit};
  return taken.data;
}
