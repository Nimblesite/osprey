// The reference host for [NATIVE-SHM-VIEW]: everything a host does, with no UI
// toolkit in the way. It calls the application's exports ([NATIVE-SHM-EVENT]),
// seals the frame every call writes, leases the view and reads every control straight out of the
// memory Osprey wrote it into. No text is parsed and nothing is copied; the
// SwiftUI and WinUI hosts do exactly this and then build native controls.
// It is also Tally's end-to-end test: `make -C src/toolchain/targets/tests tally`.
#include "libTally.h"
#include "native_bridge.h"
#include <assert.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>

enum { COLUMN, LINE, LABEL, PRESS, ENTRY, GAP };

// One leased view: its controls and their text, valid until `release_view`.
typedef struct {
  td_OwnedLease rows, text;
  const OspNativeView *controls;
  const char *chars;
  int64_t count;
} View;

static OspNativeBridge *bridge;

static const void *leased(int64_t run, td_OwnedLease *lease, int64_t *length) {
  td_OwnedLease frame;
  int64_t revision;
  const void *bytes = NULL;
  assert(osp_native_acquire(bridge, &frame, &revision) == 0);
  assert(osp_native_child(bridge, &frame, run, lease) == 0);
  assert(osp_native_release(bridge, &frame) == 0);
  assert(osp_native_length(bridge, lease, length) == 0);
  assert(osp_native_view(bridge, lease, run + 1, 0, *length, &bytes) == 0);
  return bytes;
}

// Seals the whole frame the last export wrote and leases it: a refused event
// answers nonzero and still draws, the same view, so the host always shows it.
static View sealed(int64_t status, int64_t expected) {
  View view = {0};
  int64_t revision, bytes;
  assert(status == expected);
  assert(osp_native_frame_commit(bridge, &revision) == 0);
  view.controls = leased(OSP_NATIVE_VIEWS, &view.rows, &bytes);
  view.count = bytes / (int64_t)sizeof *view.controls;
  view.chars = leased(OSP_NATIVE_TEXT, &view.text, &bytes);
  return view;
}

static void release_view(View *view) {
  assert(osp_native_release(bridge, &view->rows) == 0 &&
         osp_native_release(bridge, &view->text) == 0);
}

static bool reads(const View *view, int64_t row, const char *words) {
  const OspNativeView *control = &view->controls[row];
  return control->text_length == (int64_t)strlen(words) &&
         memcmp(view->chars + control->text_offset, words, strlen(words)) == 0;
}

// One control event of kind 0: numbers and borrowed text cross the boundary.
static int64_t control(int64_t ordinal, const char *words) {
  return osprey_app_event(1, 0, ordinal, words, "", 0, 0, false, false);
}

// A tap or an edit of the control at `row`.
static View after(View *shown, int64_t row, const char *words) {
  int64_t action = shown->controls[row].action;
  release_view(shown);
  return sealed(control(action, words), 0);
}

// Every frame of Tally has the same ten controls.
static void check_first_counts(const View *view) { assert(view->count == 10); }

static void check_first(const View *view) {
  static const int64_t kinds[] = {COLUMN, LABEL, LABEL, LINE,  PRESS,
                                  PRESS,  PRESS, ENTRY, GAP,   PRESS};
  static const int64_t parents[] = {-1, 0, 0, 0, 3, 3, 3, 0, 0, 0};
  static const int64_t actions[] = {-1, -1, -1, -1, 0, 1, 2, 3, -1, 4};
  assert(view->count == 10);
  for (int64_t row = 0; row < view->count; row++)
    assert(view->controls[row].kind == kinds[row] &&
           view->controls[row].parent == parents[row] &&
           view->controls[row].action == actions[row]);
  assert(reads(view, 1, "Tally") && reads(view, 2, "0") &&
         reads(view, 5, "Add one") && reads(view, 9, "Use night colours"));
  assert(view->controls[0].background == 0xFBFAF7FF &&
         view->controls[0].padding == 32 && view->controls[0].spacing == 18 &&
         view->controls[2].colour == 0x0B6E8AFF &&
         view->controls[2].size == 72 && view->controls[2].weight == 800 &&
         view->controls[5].background == 0x0B6E8AFF &&
         view->controls[5].radius == 10);
}

// Counting, renaming and restyling, each read back from the shared frame.
static void check_journey(View *view) {
  *view = after(view, 5, "");
  *view = after(view, 5, "");
  *view = after(view, 5, "");
  assert(view->count == 10 && reads(view, 2, "3"));
  *view = after(view, 4, "");
  assert(reads(view, 2, "2"));
  *view = after(view, 7, "Zo\xc3\xab");
  assert(reads(view, 1, "Zo\xc3\xab's tally") && reads(view, 7, "Zo\xc3\xab") &&
         reads(view, 2, "2"));
  *view = after(view, 9, "");
  assert(view->controls[0].background == 0x10141FFF &&
         view->controls[2].colour == 0x7AD7F0FF &&
         reads(view, 9, "Use day colours") && reads(view, 2, "2"));
  *view = after(view, 6, "");
  assert(reads(view, 2, "0"));
}

// An event that names no drawn control, or a kind Tally does not take, changes
// nothing: the call answers 1 and draws the same view again.
static void check_refusal(View *view) {
  const int64_t refused[][2] = {{0, 99}, {0, -1}, {2, 0}, {10, 0}};
  for (int i = 0; i < 4; i++) {
    release_view(view);
    *view = sealed(osprey_app_event(1, refused[i][0], refused[i][1], "x", "",
                                    0, 0, false, false),
                   1);
    check_first_counts(view);
    assert(reads(view, 2, "0") && reads(view, 9, "Use day colours"));
  }
}

int main(void) {
  td_ApplicationInstance instance = {{7}};
  assert(osp_native_open(&instance, TD_SHARED_FINGERPRINT, 1 << 20, 64, 64,
                         &bridge) == 0);
  assert(osprey_main() == 0);
  assert(osprey_app_argument("--unused") == 0);
  View view = sealed(osprey_app_open(1, 0, "macos", "/home/tally", false), 0);
  check_first(&view);
  check_journey(&view);
  check_refusal(&view);
  release_view(&view);
  assert(osp_native_close(bridge) == 0 && osp_native_destroy(bridge) == 0);
  puts("[ok] tally reference host: 13 views read in place from shared memory");
  return 0;
}
