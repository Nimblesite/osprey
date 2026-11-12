// Implements [NATIVE-SHM-OWNERSHIP/PUBLISH]. Internal allocator API; host
// cutover and generated language owners are separate from this storage
// substrate.
#ifndef OSPREY_NATIVE_BRIDGE_H
#define OSPREY_NATIVE_BRIDGE_H
#include "native_abi.h"
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct OspNativeBridge OspNativeBridge;
#define OSP_NATIVE_OK 0
#define OSP_NATIVE_ERROR(name) (1 + TD_BridgeError_##name##_TAG)

// Status is zero or one plus the generated BridgeError discriminant. Outputs
// remain untouched on ordinary refusal; synchronization failure is fail-stop
// (the instance becomes unusable and keeps storage pinned). Destruction
// requires the application executor and all callers to have joined; a failed
// close never frees outstanding leases.
int osp_native_open(const td_ApplicationInstance *instance,
                    const char *fingerprint, int64_t bytes, int64_t chunks,
                    int64_t owners, OspNativeBridge **out);
int osp_native_create(OspNativeBridge *bridge, int64_t type, bool leaf,
                      const void *data, int64_t length,
                      const td_OwnedLease *children, size_t count,
                      td_OwnedLease *out);
int osp_native_retain(OspNativeBridge *bridge, const td_OwnedLease *lease,
                      td_OwnedLease *out);
int osp_native_release(OspNativeBridge *bridge, const td_OwnedLease *lease);
// The pointer is borrowed from this specific lease: do not release that owner
// during a borrow. Retain a distinct owner for work on another thread/GPU.
int osp_native_view(OspNativeBridge *bridge, const td_OwnedLease *lease,
                    int64_t type, int64_t offset, int64_t length,
                    const void **out);
// Success consumes the candidate owner. Only sealed, complete roots belong
// here; atomic application-state/command commits are not implemented by this
// API.
int osp_native_publish(OspNativeBridge *bridge, const td_OwnedLease *candidate,
                       int64_t *revision);
int osp_native_acquire(OspNativeBridge *bridge, td_OwnedLease *out,
                       int64_t *revision);
int osp_native_close(OspNativeBridge *bridge);
int osp_native_destroy(OspNativeBridge *bridge);
int osp_native_usage(OspNativeBridge *bridge, size_t *bytes, size_t *chunks,
                     size_t *owners);
// Takes ownership of `data` (from malloc, `capacity` bytes) whatever the
// result, and seals its first `length` bytes as a leaf chunk without copying.
int osp_native_adopt(OspNativeBridge *bridge, int64_t type, void *data,
                     int64_t length, int64_t capacity, td_OwnedLease *out);
int osp_native_length(OspNativeBridge *bridge, const td_OwnedLease *lease,
                      int64_t *length);

// [NATIVE-SHM-SCENE] One streamed shape, laid out as the generated SceneShape.
typedef struct {
  int64_t code, index;
  double values[5]; // x, y, width (a disc's radius), height, angle
  int64_t colour, depth;
} OspNativeShape;
_Static_assert(sizeof(OspNativeShape) == sizeof(td_SceneShape) &&
                   offsetof(OspNativeShape, values) == TD_SceneShape_x_OFFSET &&
                   offsetof(OspNativeShape, colour) ==
                       TD_SceneShape_colour_OFFSET &&
                   offsetof(OspNativeShape, depth) == TD_SceneShape_depth_OFFSET,
               "OspNativeShape matches the generated SceneShape layout");
// [NATIVE-SHM-VIEW] One native control, laid out as the generated ViewRow.
typedef struct {
  int64_t kind, parent, action, text_offset, text_length, colour, background;
  double size, padding, spacing, radius;
  int64_t weight;
} OspNativeView;
_Static_assert(sizeof(OspNativeView) == sizeof(td_ViewRow) &&
                   offsetof(OspNativeView, text_offset) ==
                       TD_ViewRow_textOffset_OFFSET &&
                   offsetof(OspNativeView, size) == TD_ViewRow_size_OFFSET &&
                   offsetof(OspNativeView, weight) == TD_ViewRow_weight_OFFSET,
               "OspNativeView matches the generated ViewRow layout");
// A published frame is one branch chunk whose children are its three sealed
// runs, in this order; the value is both the child index and, plus one, the
// chunk type.
enum { OSP_NATIVE_SHAPES, OSP_NATIVE_VIEWS, OSP_NATIVE_TEXT, OSP_NATIVE_RUNS };
#define OSP_NATIVE_SCENE_SHAPES (OSP_NATIVE_SHAPES + 1)
#define OSP_NATIVE_VIEW_ROWS (OSP_NATIVE_VIEWS + 1)
#define OSP_NATIVE_VIEW_TEXT (OSP_NATIVE_TEXT + 1)
#define OSP_NATIVE_FRAME (OSP_NATIVE_RUNS + 1)
#define OSP_NATIVE_SCENE_ROWS (1 << 20) // The most shapes one call may send.
#define OSP_NATIVE_VIEW_LIMIT (1 << 16) // The most controls in one view.
#define OSP_NATIVE_TEXT_LIMIT (1 << 24) // The most text bytes in one view.
// Appends one shape to the pending run; the application executor calls this
// in drawing order. Nonzero once the run cannot grow: it is then refused whole.
int64_t osp_native_scene_shape(int64_t code, int64_t index, double x, double y,
                               double width, double height, double angle,
                               int64_t colour, int64_t depth);
// Appends one control and copies its text once into the text run. Returns the
// row's index for its children to name as `parent`, or -1 once the view is
// refused: a parent that is not an earlier row, or a run that cannot grow.
int64_t osp_native_view_row(int64_t kind, int64_t parent, int64_t action,
                            const char *text, int64_t colour,
                            int64_t background, double size, double padding,
                            double spacing, double radius, int64_t weight);
// Hands one pending run's storage to the caller and starts an empty run.
// `complete` is false when a write was refused. Executor thread only.
void *osp_native_run_take(int64_t run, int64_t *length, int64_t *capacity,
                          bool *complete);
// Seals the three pending runs in place and publishes them as one frame; a
// refused run publishes nothing. Call on the executor thread after each
// application export returns.
int osp_native_frame_commit(OspNativeBridge *bridge, int64_t *revision);
// Drops the three pending runs: for a call that failed, whose partial frame
// belongs to nothing. Executor thread only.
void osp_native_frame_discard(void);
// A new owner of the `index`th child of a branch chunk.
int osp_native_child(OspNativeBridge *bridge, const td_OwnedLease *lease,
                     int64_t index, td_OwnedLease *out);
#endif
