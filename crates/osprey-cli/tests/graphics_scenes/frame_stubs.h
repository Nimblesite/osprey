#include <stdint.h>
#include <stdio.h>
#include <string.h>
typedef int HRESULT;
typedef unsigned UINT;
typedef unsigned DWORD;
typedef void ID3D12CommandList;
typedef struct { float slot[24], res[2], time, pad; } OspGfxUniforms;
typedef struct {
  void *queue, *fence, *fenceEvent, *commands, *swap, *allocator, *pipeline;
  uint64_t fenceValue;
  int closed;
} OspGfxContext;
#define OSP_GFX_API
#define OSP_GFX_VSYNC_INTERVAL 1u
#define OSP_GFX_FRAME_TIMEOUT_MS 5000u
#define WAIT_OBJECT_0 0u
#define WAIT_TIMEOUT 258u
#define WAIT_FAILED UINT32_MAX
enum { NONE, ALLOCATOR, COMMANDS, CLOSE, PRESENT, SIGNAL, EVENT, TIMEOUT, WAIT_ERROR, REMOVED, EXECUTE, RECORD, WAIT, DESTROY, STAGES };
static int failure, calls[STAGES];
static HRESULT outcome(int stage) { calls[stage]++; return failure == stage ? -1 : 0; }
static uint64_t completion(void) { return failure == REMOVED ? UINT64_MAX : 0; }
static DWORD fence_wait(void) {
  calls[WAIT]++;
  return failure == TIMEOUT ? WAIT_TIMEOUT : failure == WAIT_ERROR ? WAIT_FAILED : WAIT_OBJECT_0;
}
static int osp_gfx_ok(HRESULT status, const char *operation) { (void)operation; return status >= 0; }
static OspGfxUniforms osp_gfx_frame_uniforms(OspGfxContext *ctx) { (void)ctx; OspGfxUniforms u = {0}; return u; }
static void osp_gfx_record(OspGfxContext *ctx, UINT index, const OspGfxUniforms *u) { (void)ctx; (void)index; (void)u; calls[RECORD]++; }
static void osp_gfx_pump_events(OspGfxContext *ctx) { (void)ctx; }
static void osp_gfx_destroy(OspGfxContext *ctx) { (void)ctx; calls[DESTROY]++; }
#define ID3D12CommandAllocator_Reset(a) ((void)(a), outcome(ALLOCATOR))
#define ID3D12GraphicsCommandList_Reset(c, a, p) ((void)(c), (void)(a), (void)(p), outcome(COMMANDS))
#define ID3D12GraphicsCommandList_Close(c) ((void)(c), outcome(CLOSE))
#define ID3D12CommandQueue_ExecuteCommandLists(q, n, l) ((void)(q), (void)(n), (void)(l), (void)outcome(EXECUTE))
#define IDXGISwapChain3_Present(s, v, f) ((void)(s), (void)(v), (void)(f), outcome(PRESENT))
#define IDXGISwapChain3_GetCurrentBackBufferIndex(s) ((void)(s), 0u)
#define ID3D12CommandQueue_Signal(q, f, v) ((void)(q), (void)(f), (void)(v), outcome(SIGNAL))
#define ID3D12Fence_GetCompletedValue(f) ((void)(f), completion())
#define ID3D12Fence_SetEventOnCompletion(f, v, e) ((void)(f), (void)(v), (void)(e), outcome(EVENT))
#define WaitForSingleObject(e, t) ((void)(e), (void)(t), fence_wait())
#define CHECK(condition) do { if (!(condition)) { fprintf(stderr, "stage %d failed line %d: %s\n", failure, __LINE__, #condition); return 0; } } while (0)
