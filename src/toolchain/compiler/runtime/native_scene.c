// [NATIVE-SHM-SCENE] [NATIVE-SHM-VIEW] The pending runs of the one application
// executor: streamed shapes, native controls and their text. Each value is
// written once, here, into the storage a host later reads in place: no host
// call and no encoding per value. Portable C only, so the wasm runtime carries
// it too.
#include "native_bridge.h"
#include <math.h>
#include <stdatomic.h>
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

// [NATIVE-SHM-SAFETY] Fibers are OS threads, so two may write at once. One
// flag makes each row, and the taking of all runs, a single step; it is
// uncontended on the one executor, and held only for a row's few stores.
static atomic_flag writing = ATOMIC_FLAG_INIT;
static void runs_enter(void) {
  while (atomic_flag_test_and_set_explicit(&writing, memory_order_acquire))
    ;
}
static void runs_leave(void) {
  atomic_flag_clear_explicit(&writing, memory_order_release);
}
// A host lays these out: a value that is not finite is never published.
static bool finite(const double *values, int count) {
  for (int i = 0; i < count; i++)
    if (!isfinite(values[i]))
      return false;
  return true;
}
// Marks `run` refused, so the frame it belongs to publishes nothing.
static int64_t run_refuse(NativeRun *run, int64_t result) {
  run->refused = true;
  runs_leave();
  return result;
}

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
  const double values[] = {x, y, width, height, angle};
  runs_enter();
  NativeRun *shapes = &runs[OSP_NATIVE_SHAPES];
  OspNativeShape *row =
      finite(values, 5) ? run_reserve(shapes, sizeof *row) : NULL;
  if (!row)
    return run_refuse(shapes, 1);
  *row = (OspNativeShape){
      code, index, {x, y, width, height, angle}, colour, depth};
  runs_leave();
  return 0;
}

int64_t osp_native_view_row(int64_t kind, int64_t parent, int64_t action,
                            const char *text, int64_t colour,
                            int64_t background, double size, double padding,
                            double spacing, double radius, int64_t weight) {
  const double extents[] = {size, padding, spacing, radius};
  size_t bytes = text ? strlen(text) : 0;
  bool usable = finite(extents, 4) && size >= 0 && padding >= 0 &&
                spacing >= 0 && radius >= 0;
  runs_enter();
  NativeRun *views = &runs[OSP_NATIVE_VIEWS], *chars = &runs[OSP_NATIVE_TEXT];
  int64_t index = (int64_t)(views->length / sizeof(OspNativeView));
  size_t offset = chars->length;
  char *copy = !usable || parent < -1 || parent >= index
                   ? NULL
                   : run_reserve(chars, bytes);
  OspNativeView *row = copy ? run_reserve(views, sizeof *row) : NULL;
  if (!row)
    return run_refuse(views, -1);
  if (bytes)
    memcpy(copy, text, bytes);
  *row = (OspNativeView){kind,    parent,  action,  (int64_t)offset,
                         (int64_t)bytes, colour, background, size,
                         padding, spacing, radius,  weight};
  runs_leave();
  return index;
}

bool osp_native_runs_take(void *data[OSP_NATIVE_RUNS],
                          int64_t length[OSP_NATIVE_RUNS],
                          int64_t capacity[OSP_NATIVE_RUNS]) {
  bool complete = true;
  runs_enter();
  for (int run = 0; run < OSP_NATIVE_RUNS; run++) {
    data[run] = runs[run].data;
    length[run] = (int64_t)runs[run].length;
    capacity[run] = (int64_t)runs[run].capacity;
    complete = complete && !runs[run].refused;
    runs[run] = (NativeRun){.limit = runs[run].limit};
  }
  runs_leave();
  return complete;
}

void osp_native_frame_discard(void) {
  void *data[OSP_NATIVE_RUNS];
  int64_t length[OSP_NATIVE_RUNS], capacity[OSP_NATIVE_RUNS];
  (void)osp_native_runs_take(data, length, capacity);
  for (int run = 0; run < OSP_NATIVE_RUNS; run++)
    free(data[run]);
}
