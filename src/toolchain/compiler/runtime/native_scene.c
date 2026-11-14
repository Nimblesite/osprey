// [NATIVE-SHM-SCENE] [NATIVE-SHM-VIEW] [NATIVE-SHM-ASK] The pending runs of the
// one application executor: streamed shapes, native controls, the requests and
// slice writes they ask of the host, and their text. Each value is
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
    [OSP_NATIVE_ASKS] = {.limit = OSP_NATIVE_ASK_LIMIT * sizeof(OspNativeAsk)},
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
// Bionic's math.h declares `finite`, so this helper carries its own name.
static bool all_finite(const double *values, int count) {
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
                               int64_t colour, int64_t depth, const char *text) {
  const double values[] = {x, y, width, height, angle};
  size_t bytes = text ? strlen(text) : 0;
  runs_enter();
  NativeRun *shapes = &runs[OSP_NATIVE_SHAPES], *chars = &runs[OSP_NATIVE_TEXT];
  size_t offset = chars->length;
  char *copy = all_finite(values, 5) ? run_reserve(chars, bytes) : NULL;
  OspNativeShape *row = copy ? run_reserve(shapes, sizeof *row) : NULL;
  if (!row)
    return run_refuse(shapes, 1);
  if (bytes)
    memcpy(copy, text, bytes);
  *row = (OspNativeShape){.code = code,
                          .index = index,
                          .values = {x, y, width, height, angle},
                          .colour = colour,
                          .depth = depth,
                          .text_offset = (int64_t)offset,
                          .text_length = (int64_t)bytes};
  runs_leave();
  return 0;
}

int64_t osp_native_view_row(int64_t kind, int64_t parent, int64_t action,
                            const char *text, int64_t colour,
                            int64_t background, double size, double padding,
                            double spacing, double radius, int64_t weight) {
  const double extents[] = {size, padding, spacing, radius};
  size_t bytes = text ? strlen(text) : 0;
  bool usable = all_finite(extents, 4) && size >= 0 && padding >= 0 &&
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
  *row = (OspNativeView){.kind = kind,
                         .parent = parent,
                         .action = action,
                         .text_offset = (int64_t)offset,
                         .text_length = (int64_t)bytes,
                         .colour = colour,
                         .background = background,
                         .size = size,
                         .padding = padding,
                         .spacing = spacing,
                         .radius = radius,
                         .weight = weight};
  runs_leave();
  return index;
}

int64_t osp_native_view_extend(int64_t row, double width, double height,
                               double grow, int64_t flags, int64_t border,
                               const char *hint) {
  const double extents[] = {width, height, grow};
  size_t bytes = hint ? strlen(hint) : 0;
  bool usable = all_finite(extents, 3) && width >= 0 && height >= 0 && grow >= 0;
  runs_enter();
  NativeRun *views = &runs[OSP_NATIVE_VIEWS], *chars = &runs[OSP_NATIVE_TEXT];
  size_t offset = chars->length;
  bool known = usable && !views->refused && row >= 0 &&
               (size_t)row < views->length / sizeof(OspNativeView);
  char *copy = known ? run_reserve(chars, bytes) : NULL;
  if (!copy)
    return run_refuse(views, -1);
  if (bytes)
    memcpy(copy, hint, bytes);
  // A control may be fitted twice (its own state, then its wrapper's): flags
  // add up and every other value that is set replaces the one before.
  OspNativeView *view = (OspNativeView *)(void *)views->data + row;
  view->width = width > 0 ? width : view->width;
  view->height = height > 0 ? height : view->height;
  view->grow = grow > 0 ? grow : view->grow;
  view->flags |= flags;
  view->border = border ? border : view->border;
  view->hint_offset = bytes ? (int64_t)offset : view->hint_offset;
  view->hint_length = bytes ? (int64_t)bytes : view->hint_length;
  runs_leave();
  return 0;
}

int64_t osp_native_ask(int64_t field, int64_t number, const char *text) {
  size_t bytes = text ? strlen(text) : 0;
  runs_enter();
  NativeRun *asks = &runs[OSP_NATIVE_ASKS], *chars = &runs[OSP_NATIVE_TEXT];
  size_t offset = chars->length;
  char *copy = field < 0 ? NULL : run_reserve(chars, bytes);
  OspNativeAsk *row = copy ? run_reserve(asks, sizeof *row) : NULL;
  if (!row)
    return run_refuse(asks, 1);
  if (bytes)
    memcpy(copy, text, bytes);
  *row = (OspNativeAsk){.field = field,
                        .number = number,
                        .text_offset = (int64_t)offset,
                        .text_length = (int64_t)bytes};
  runs_leave();
  return 0;
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
