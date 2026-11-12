// [NATIVE-SHM-SCENE] The pending shape run of the one application executor.
// Shapes are written once, here, into the storage a host later reads in
// place: no lock, no host call and no copy per shape. Portable C only, so the
// wasm runtime carries it too.
#include "native_bridge.h"
#include <stdlib.h>

enum { SCENE_FIRST_ROWS = 256 };
static OspNativeShape *rows;
static size_t count, capacity;
static bool refused;

static bool scene_grow(void) {
  size_t next = capacity ? capacity * 2 : SCENE_FIRST_ROWS;
  OspNativeShape *grown = next > OSP_NATIVE_SCENE_ROWS
                              ? NULL
                              : realloc(rows, next * sizeof *rows);
  if (!grown)
    return false;
  rows = grown;
  capacity = next;
  return true;
}

int64_t osp_native_scene_shape(int64_t code, int64_t index, double x, double y,
                               double width, double height, double angle,
                               int64_t colour, int64_t depth) {
  if (refused || (count == capacity && !scene_grow())) {
    refused = true;
    return 1;
  }
  rows[count++] = (OspNativeShape){
      code, index, {x, y, width, height, angle}, colour, depth};
  return 0;
}

void *osp_native_scene_take(int64_t *length, int64_t *bytes, bool *complete) {
  void *taken = rows;
  *length = (int64_t)(count * sizeof *rows);
  *bytes = (int64_t)(capacity * sizeof *rows);
  *complete = !refused;
  rows = NULL;
  count = capacity = 0;
  refused = false;
  return taken;
}
