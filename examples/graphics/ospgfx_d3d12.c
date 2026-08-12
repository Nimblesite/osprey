// The Osprey-facing half of the Direct3D 12 backend: the six exported symbols
// and the per-frame path. Everything they are built out of lives in
// ospgfx_d3d12_setup.c, and ospgfx_d3d12.h explains the split, mirrors the
// uniform ABI, and carries the notice that none of this backend has ever been
// compiled on the machine that wrote it.
//
// These six functions are the whole contract with Osprey, and they mean exactly
// what ospgfx.m's do: values arrive in 4096ths or thousandths, out-of-range
// slots are ignored, `osp_gfx_draw` returns 1 while the window lives and 0 once
// it closes, and `osp_gfx_ticks` is milliseconds since the window opened.

#include "ospgfx_d3d12.h"

#include <stdlib.h>

static const FLOAT OSP_GFX_CLEAR[4] = {0.0f, 0.0f, 0.0f, 1.0f};

// A window Osprey could not possibly have meant is refused, but a nonsensical
// scale is clamped rather than refused — the same asymmetry ospgfx.m has, and
// for the same reason: the resolution is a contract, the magnification is not.
static int osp_gfx_extent_ok(int64_t width, int64_t height) {
  return width > 0 && height > 0 && width <= OSP_GFX_MAX_EXTENT &&
         height <= OSP_GFX_MAX_EXTENT;
}

static int64_t osp_gfx_clamp_scale(int64_t scale) {
  return scale < OSP_GFX_MIN_SCALE || scale > OSP_GFX_MAX_SCALE ? OSP_GFX_MIN_SCALE : scale;
}

// The bring-up, in order, short-circuiting on the first refusal — which has
// already said what it was. The factory is the one object not kept: the swap
// chain holds what it needs, so it is released whether or not this succeeded.
static int osp_gfx_build(OspGfxContext *ctx, int64_t width, int64_t height,
                         const char *title, const char *shaderPath,
                         const char *fragmentName) {
  IDXGIFactory4 *factory = NULL;
  int ready = osp_gfx_make_window(ctx, width, height, title) &&
              osp_gfx_device(ctx, &factory) && osp_gfx_swapchain(ctx, factory) &&
              osp_gfx_targets(ctx) && osp_gfx_pipeline(ctx, shaderPath, fragmentName) &&
              osp_gfx_frame_objects(ctx);
  osp_gfx_release(factory);
  return ready;
}

// `scale` magnifies the design resolution into window points: 480x300 at
// scale 2 opens a 960x600 window, multiplied again by the display's DPI so a
// high-density screen is shaded at its full pixel count. `fragmentName` picks
// one entry out of the shared library resolved from `shaderPath`.
OSP_GFX_API void *osp_gfx_open(int64_t width, int64_t height, int64_t scale,
                               const char *title, const char *shaderPath,
                               const char *fragmentName) {
  OspGfxContext *ctx;
  if (!osp_gfx_extent_ok(width, height)) {
    return NULL;
  }
  scale = osp_gfx_clamp_scale(scale);
  ctx = (OspGfxContext *)calloc(1, sizeof *ctx);
  if (!ctx) {
    return NULL;
  }
  ctx->openedAt = osp_gfx_now();
  if (osp_gfx_build(ctx, width * scale, height * scale, title, shaderPath, fragmentName)) {
    return ctx;
  }
  osp_gfx_destroy(ctx);
  return NULL;
}

// Store one slot after dividing `value` by `scale`. Out-of-range slots are
// ignored rather than trapping, so a scene bug never takes the process down.
static int64_t osp_gfx_store(void *handle, int64_t slot, int64_t value, float scale) {
  OspGfxContext *ctx = (OspGfxContext *)handle;
  if (!ctx || slot < 0 || slot >= OSP_GFX_SLOTS) {
    return 0;
  }
  ctx->uniforms.slot[slot] = (float)value / scale;
  return 1;
}

// Push one moving scene parameter, in 4096ths; the shader sees a float.
OSP_GFX_API int64_t osp_gfx_set(void *handle, int64_t slot, int64_t value) {
  return osp_gfx_store(handle, slot, value, OSP_GFX_FIXED_ONE);
}

// Push one authored tuning constant, in thousandths. Dividing an exact integer
// by 1000 rounds to the nearest float, which is the same float the shader would
// have held had the number stayed written in its source — so moving a constant
// out of the shader and into Osprey is a refactor, not a re-grade.
OSP_GFX_API int64_t osp_gfx_set_milli(void *handle, int64_t slot, int64_t thousandths) {
  return osp_gfx_store(handle, slot, thousandths, OSP_GFX_MILLI_ONE);
}

static void osp_gfx_pump_events(OspGfxContext *ctx) {
  MSG message;
  while (PeekMessageW(&message, NULL, 0, 0, PM_REMOVE)) {
    if (message.message == WM_QUIT) {
      ctx->closed = 1;
    }
    (void)TranslateMessage(&message);
    (void)DispatchMessageW(&message);
  }
}

// Resolution and elapsed time are the two uniforms the host fills in rather
// than Osprey: only the swap chain knows how big the client area turned out at
// this display's density, and the clock has moved since the last frame.
static OspGfxUniforms osp_gfx_frame_uniforms(OspGfxContext *ctx) {
  ctx->uniforms.res[0] = (float)ctx->width;
  ctx->uniforms.res[1] = (float)ctx->height;
  ctx->uniforms.time = (float)(osp_gfx_now() - ctx->openedAt);
  return ctx->uniforms;
}

static void osp_gfx_barrier(OspGfxContext *ctx, ID3D12Resource *target,
                            D3D12_RESOURCE_STATES from, D3D12_RESOURCE_STATES to) {
  D3D12_RESOURCE_BARRIER barrier;
  ZeroMemory(&barrier, sizeof barrier);
  barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
  barrier.Transition.pResource = target;
  barrier.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
  barrier.Transition.StateBefore = from;
  barrier.Transition.StateAfter = to;
  ID3D12GraphicsCommandList_ResourceBarrier(ctx->commands, 1, &barrier);
}

// Direct3D has no implicit full-target viewport the way a Metal render pass
// descriptor does, so the whole client area is set explicitly every frame.
static void osp_gfx_set_viewport(OspGfxContext *ctx) {
  D3D12_VIEWPORT view;
  D3D12_RECT scissor;
  view.TopLeftX = 0.0f;
  view.TopLeftY = 0.0f;
  view.Width = (float)ctx->width;
  view.Height = (float)ctx->height;
  view.MinDepth = 0.0f;
  view.MaxDepth = 1.0f;
  scissor.left = 0;
  scissor.top = 0;
  scissor.right = (LONG)ctx->width;
  scissor.bottom = (LONG)ctx->height;
  ID3D12GraphicsCommandList_RSSetViewports(ctx->commands, 1, &view);
  ID3D12GraphicsCommandList_RSSetScissorRects(ctx->commands, 1, &scissor);
}

// One full-screen triangle, shaded by the selected fragment entry from the
// slots Osprey pushed. The uniforms go in as root constants, written straight
// into the command list, so no buffer outlives the frame that used it.
static void osp_gfx_record(OspGfxContext *ctx, UINT index, const OspGfxUniforms *u) {
  ID3D12GraphicsCommandList *cmd = ctx->commands;
  osp_gfx_barrier(ctx, ctx->targets[index], D3D12_RESOURCE_STATE_PRESENT,
                  D3D12_RESOURCE_STATE_RENDER_TARGET);
  ID3D12GraphicsCommandList_OMSetRenderTargets(cmd, 1, &ctx->rtvs[index], FALSE, NULL);
  ID3D12GraphicsCommandList_ClearRenderTargetView(cmd, ctx->rtvs[index], OSP_GFX_CLEAR, 0,
                                                  NULL);
  osp_gfx_set_viewport(ctx);
  ID3D12GraphicsCommandList_SetGraphicsRootSignature(cmd, ctx->root);
  ID3D12GraphicsCommandList_SetGraphicsRoot32BitConstants(cmd, 0, OSP_GFX_ROOT_CONSTANTS,
                                                          u, 0);
  ID3D12GraphicsCommandList_IASetPrimitiveTopology(cmd, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
  ID3D12GraphicsCommandList_DrawInstanced(cmd, OSP_GFX_TRIANGLE_VERTICES, 1, 0, 0);
  osp_gfx_barrier(ctx, ctx->targets[index], D3D12_RESOURCE_STATE_RENDER_TARGET,
                  D3D12_RESOURCE_STATE_PRESENT);
}

// Wait for the frame just submitted to finish. Osprey calls osp_gfx_draw once
// per frame and expects it to mean "this frame is on screen", so one allocator
// and a full sync is the honest shape — the same one-frame-at-a-time cadence
// `nextDrawable` imposes on the Metal bridge.
static int osp_gfx_wait(OspGfxContext *ctx) {
  HRESULT signalled;
  ctx->fenceValue++;
  signalled = ID3D12CommandQueue_Signal(ctx->queue, ctx->fence, ctx->fenceValue);
  if (!osp_gfx_ok(signalled, "Signal")) {
    return 0;
  }
  if (ID3D12Fence_GetCompletedValue(ctx->fence) >= ctx->fenceValue) {
    return 1;
  }
  signalled = ID3D12Fence_SetEventOnCompletion(ctx->fence, ctx->fenceValue, ctx->fenceEvent);
  if (!osp_gfx_ok(signalled, "SetEventOnCompletion")) {
    return 0;
  }
  return WaitForSingleObject(ctx->fenceEvent, OSP_GFX_FRAME_TIMEOUT_MS) == WAIT_OBJECT_0;
}

static void osp_gfx_submit(OspGfxContext *ctx) {
  ID3D12CommandList *lists[1];
  lists[0] = (ID3D12CommandList *)ctx->commands;
  ID3D12CommandQueue_ExecuteCommandLists(ctx->queue, 1, lists);
  (void)osp_gfx_ok(IDXGISwapChain3_Present(ctx->swap, OSP_GFX_VSYNC_INTERVAL, 0), "Present");
  (void)osp_gfx_wait(ctx);
}

// Reset this frame's recording, shade every pixel, and hand it to the queue. A
// failure anywhere leaves the previous frame on screen instead of tearing the
// process down, and has already printed which call refused.
static void osp_gfx_frame(OspGfxContext *ctx, UINT index) {
  OspGfxUniforms u = osp_gfx_frame_uniforms(ctx);
  if (!osp_gfx_ok(ID3D12CommandAllocator_Reset(ctx->allocator), "allocator Reset") ||
      !osp_gfx_ok(ID3D12GraphicsCommandList_Reset(ctx->commands, ctx->allocator,
                                                  ctx->pipeline),
                  "command list Reset")) {
    return;
  }
  osp_gfx_record(ctx, index, &u);
  if (osp_gfx_ok(ID3D12GraphicsCommandList_Close(ctx->commands), "Close")) {
    osp_gfx_submit(ctx);
  }
}

// Shade every pixel from the current slots and present. Returns 1 while the
// window is open, 0 once the user has closed it.
OSP_GFX_API int64_t osp_gfx_draw(void *handle) {
  OspGfxContext *ctx = (OspGfxContext *)handle;
  if (!ctx || ctx->closed) {
    return 0;
  }
  osp_gfx_frame(ctx, IDXGISwapChain3_GetCurrentBackBufferIndex(ctx->swap));
  osp_gfx_pump_events(ctx);
  return ctx->closed ? 0 : 1;
}

// Milliseconds since the window opened. Animations advance on wall-clock time
// so they play at the same speed whatever frame rate the host sustains.
OSP_GFX_API int64_t osp_gfx_ticks(void *handle) {
  OspGfxContext *ctx = (OspGfxContext *)handle;
  return ctx ? (int64_t)((osp_gfx_now() - ctx->openedAt) * 1000.0) : 0;
}

// Every GPU object here is still referenced by work in flight, so the queue is
// drained before anything is released — the Metal bridge gets that for free
// from ARC and from a command buffer's own retain of what it touched.
OSP_GFX_API int64_t osp_gfx_close(void *handle) {
  OspGfxContext *ctx = (OspGfxContext *)handle;
  if (!ctx) {
    return 0;
  }
  (void)osp_gfx_wait(ctx);
  osp_gfx_destroy(ctx);
  return 1;
}
