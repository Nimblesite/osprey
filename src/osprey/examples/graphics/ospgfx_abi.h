// The Osprey-facing contract every graphics bridge implements, written down
// ONCE. `base/base.osp` declares six `extern fn`s and is byte-identical on
// every platform; this header is the C side of that agreement, shared by the
// Swift Metal host (macos/, through its bridging header) and the Direct3D 12
// bridge (ospgfx_d3d12.{h,c}) so that neither can drift from the other on the
// slot count, the two fixed-point scales, the uniform block or the argument
// validation.
//
// The shader libraries (base.metal, base.hlsl) cannot include this header —
// both bridges compile their shader from source at run time with no include
// path — so each spells `OSP_GFX_SLOTS` and the uniform layout out again, and
// src/toolchain/crates/osprey-cli/tests/graphics_scenes.rs asserts the copies agree with
// this file. A disagreement does not fail to compile; it silently shifts `res`
// and `time` and corrupts every scene, which is why there is exactly one C
// declaration and a test rather than two declarations and hope.
//
// Everything here is platform-neutral C11 with no dependency beyond stdint.h,
// so the same text compiles under clang on macOS, MinGW/MSVC on Windows, and
// the Linux-hosted frame-path test that compiles the D3D12 frame functions
// against stubbed SDK calls.
//
// This slot protocol is also the seam where plan 0023's extracted kernels
// meet the graphics bridge: a kernel's leading uniform parameters are exactly
// a slot vector, so an emitter that targets this ABI needs no second one.

#ifndef OSPGFX_ABI_H
#define OSPGFX_ABI_H

#include <stdint.h>

// Parameter slots Osprey can push per frame.
#define OSP_GFX_SLOTS 24

// Two scales, because a slot carries one of two very different things. Motion
// arrives in fixed point (4096 = 1.0), where a full turn wraps with an integer
// modulo and Osprey's oscillators are exact. Authored tuning arrives in
// thousandths, so a number written 0.085 by a scene author divides back to
// exactly the float the literal 0.085 would have produced.
#define OSP_GFX_FIXED_ONE 4096.0f
#define OSP_GFX_MILLI_ONE 1000.0f

// The uniform block as the GPU sees it: the slots, then the drawable's
// resolution and the elapsed seconds the bridge fills in itself. Field order
// is ABI with both shader libraries.
typedef struct OspGfxUniforms {
  float slot[OSP_GFX_SLOTS];
  float res[2];
  float time;
  float pad;
} OspGfxUniforms;

// A window Osprey could not possibly have meant is refused, but a nonsensical
// scale is clamped rather than refused: the resolution is a contract, the
// magnification is not.
#define OSP_GFX_MAX_EXTENT 8192
#define OSP_GFX_MIN_SCALE 1
#define OSP_GFX_MAX_SCALE 16

// Every library exposes one vertex stage under this name and selects the
// fragment entry by the name the scene passes, defaulting to the Kali entry.
#define OSP_GFX_VERTEX_ENTRY "osp_vertex"
#define OSP_GFX_DEFAULT_FRAGMENT "osp_fragment"

static inline int osp_gfx_extent_ok(int64_t width, int64_t height) {
  return width > 0 && height > 0 && width <= OSP_GFX_MAX_EXTENT &&
         height <= OSP_GFX_MAX_EXTENT;
}

static inline int64_t osp_gfx_clamp_scale(int64_t scale) {
  return scale < OSP_GFX_MIN_SCALE || scale > OSP_GFX_MAX_SCALE ? OSP_GFX_MIN_SCALE
                                                                 : scale;
}

// The fragment entry a scene asked for, or the default when it named none.
static inline const char *osp_gfx_fragment_entry(const char *fragmentName) {
  return fragmentName && *fragmentName ? fragmentName : OSP_GFX_DEFAULT_FRAGMENT;
}

// Store one slot after dividing `value` by `scale`. Out-of-range slots are
// ignored rather than trapping, so a scene bug never takes the process down;
// the return value is how many slots were stored, which is what the Osprey
// host counts to detect a layout disagreement.
static inline int64_t osp_gfx_slot_store(OspGfxUniforms *u, int64_t slot,
                                         int64_t value, float scale) {
  if (!u || slot < 0 || slot >= OSP_GFX_SLOTS) {
    return 0;
  }
  u->slot[slot] = (float)value / scale;
  return 1;
}

// Milliseconds between two readings of a monotonic clock, for `osp_gfx_ticks`.
static inline int64_t osp_gfx_elapsed_ms(double openedAt, double now) {
  return (int64_t)((now - openedAt) * 1000.0);
}

#endif
