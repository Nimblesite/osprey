// One-time bring-up for the Direct3D 12 backend: the window, the device, the
// swap chain, the render targets, the shaders, the root signature, the pipeline
// state and the frame objects — everything `osp_gfx_open` builds once and
// everything `osp_gfx_close` gives back. The C ABI and the per-frame path are
// in ospgfx_d3d12.c; ospgfx_d3d12.h explains the split and carries the notice
// that none of this has ever been compiled on the machine that wrote it.

#include "ospgfx_d3d12.h"

#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_MSC_VER)
#pragma comment(lib, "d3d12.lib")
#pragma comment(lib, "dxgi.lib")
#pragma comment(lib, "d3dcompiler.lib")
#pragma comment(lib, "dxguid.lib")
#pragma comment(lib, "user32.lib")
#endif

// DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, spelled as its numeric value so
// this file still builds against an SDK too old to declare the type.
#define OSP_GFX_DPI_PER_MONITOR_V2 ((HANDLE)(INT_PTR)-4)

static const wchar_t OSP_GFX_CLASS[] = L"OspreyGfxWindow";

double osp_gfx_now(void) {
  LARGE_INTEGER frequency;
  LARGE_INTEGER counter;
  if (!QueryPerformanceFrequency(&frequency) || frequency.QuadPart == 0) {
    return 0.0;
  }
  (void)QueryPerformanceCounter(&counter);
  return (double)counter.QuadPart / (double)frequency.QuadPart;
}

int osp_gfx_ok(HRESULT hr, const char *what) {
  if (SUCCEEDED(hr)) {
    return 1;
  }
  fprintf(stderr, "ospgfx: %s failed (hr=0x%08lx)\n", what, (unsigned long)hr);
  return 0;
}

// Called through the interface's own vtable rather than the `IUnknown_Release`
// convenience macro, which is one fewer header the two toolchains have to agree
// about. Every COM interface begins with that pointer, so the cast is the
// standard one COM guarantees.
void osp_gfx_release(void *object) {
  IUnknown *com = (IUnknown *)object;
  if (com) {
    (void)com->lpVtbl->Release(com);
  }
}

// D3DCompile and the root-signature serialiser both report through a blob that
// is present only when something went wrong; printing and freeing it in one
// place keeps either call site from leaking it.
static void osp_gfx_report(ID3DBlob *errors, const char *what) {
  if (errors) {
    fprintf(stderr, "ospgfx: %s:\n%s\n", what,
            (const char *)ID3D10Blob_GetBufferPointer(errors));
    osp_gfx_release(errors);
  }
}

static LRESULT CALLBACK osp_gfx_proc(HWND window, UINT message, WPARAM wp, LPARAM lp) {
  OspGfxContext *ctx = (OspGfxContext *)GetWindowLongPtrW(window, GWLP_USERDATA);
  if (message == WM_NCCREATE) {
    CREATESTRUCTW *create = (CREATESTRUCTW *)lp;
    (void)SetWindowLongPtrW(window, GWLP_USERDATA, (LONG_PTR)create->lpCreateParams);
  } else if (ctx && (message == WM_CLOSE || message == WM_DESTROY)) {
    ctx->closed = 1;
  }
  return DefWindowProcW(window, message, wp, lp);
}

static int osp_gfx_register_class(void) {
  static int registered = 0;
  WNDCLASSEXW cls;
  if (registered) {
    return 1;
  }
  ZeroMemory(&cls, sizeof cls);
  cls.cbSize = sizeof cls;
  cls.lpfnWndProc = osp_gfx_proc;
  cls.hInstance = GetModuleHandleW(NULL);
  cls.hCursor = LoadCursorW(NULL, IDC_ARROW);
  cls.lpszClassName = OSP_GFX_CLASS;
  registered = RegisterClassExW(&cls) != 0;
  if (!registered) {
    fprintf(stderr, "ospgfx: cannot register window class\n");
  }
  return registered;
}

// Titles arrive as UTF-8 from Osprey and Win32 wants UTF-16. A title that will
// not convert is not worth failing an open over; fall back to the product name.
static void osp_gfx_widen(const char *utf8, wchar_t *out, int cap) {
  static const wchar_t fallback[] = L"Osprey";
  int i = 0;
  if (utf8 && *utf8 && MultiByteToWideChar(CP_UTF8, 0, utf8, -1, out, cap) > 0) {
    return;
  }
  while (i < cap - 1 && fallback[i]) {
    out[i] = fallback[i];
    i++;
  }
  out[i] = L'\0';
}

// Per-monitor DPI awareness and GetDpiForSystem are Windows 10 additions with
// no import library on an older SDK, so both are resolved at run time: this
// file builds anywhere and simply renders at 96 DPI where they are absent.
// Declaring awareness is what makes the window physical pixels rather than
// something the compositor stretches — the Windows counterpart of drawing a
// CAMetalLayer at the display's backing scale.
static UINT osp_gfx_dpi(void) {
  typedef HANDLE(WINAPI * OspGfxSetDpi)(HANDLE);
  typedef UINT(WINAPI * OspGfxGetDpi)(void);
  HMODULE user32 = GetModuleHandleW(L"user32.dll");
  OspGfxSetDpi declare =
      user32 ? (OspGfxSetDpi)(void *)GetProcAddress(user32, "SetProcessDpiAwarenessContext") : NULL;
  OspGfxGetDpi query =
      user32 ? (OspGfxGetDpi)(void *)GetProcAddress(user32, "GetDpiForSystem") : NULL;
  if (declare) {
    (void)declare(OSP_GFX_DPI_PER_MONITOR_V2);
  }
  return query ? query() : OSP_GFX_DPI_DEFAULT;
}

static RECT osp_gfx_window_rect(int64_t width, int64_t height, UINT dpi) {
  RECT rect;
  rect.left = 0;
  rect.top = 0;
  rect.right = (LONG)(width * (int64_t)dpi / (int64_t)OSP_GFX_DPI_DEFAULT);
  rect.bottom = (LONG)(height * (int64_t)dpi / (int64_t)OSP_GFX_DPI_DEFAULT);
  (void)AdjustWindowRect(&rect, OSP_GFX_WINDOW_STYLE, FALSE);
  return rect;
}

// The swap chain is sized from the client area the window actually got, never
// from what was asked for, so a DPI or border adjustment can never leave the
// shader disagreeing with the framebuffer about the resolution.
static int osp_gfx_client_size(OspGfxContext *ctx) {
  RECT client;
  if (!GetClientRect(ctx->window, &client) || client.right <= client.left ||
      client.bottom <= client.top) {
    fprintf(stderr, "ospgfx: window has no client area\n");
    return 0;
  }
  ctx->width = (UINT)(client.right - client.left);
  ctx->height = (UINT)(client.bottom - client.top);
  return 1;
}

int osp_gfx_make_window(OspGfxContext *ctx, int64_t width, int64_t height,
                        const char *title) {
  wchar_t wide[OSP_GFX_TITLE_MAX];
  RECT rect = osp_gfx_window_rect(width, height, osp_gfx_dpi());
  if (!osp_gfx_register_class()) {
    return 0;
  }
  osp_gfx_widen(title, wide, OSP_GFX_TITLE_MAX);
  ctx->window = CreateWindowExW(0, OSP_GFX_CLASS, wide, OSP_GFX_WINDOW_STYLE, CW_USEDEFAULT,
                                CW_USEDEFAULT, rect.right - rect.left, rect.bottom - rect.top,
                                NULL, NULL, GetModuleHandleW(NULL), ctx);
  if (!ctx->window) {
    fprintf(stderr, "ospgfx: cannot create window\n");
    return 0;
  }
  (void)ShowWindow(ctx->window, SW_SHOW);
  (void)SetForegroundWindow(ctx->window);
  return osp_gfx_client_size(ctx);
}

int osp_gfx_device(OspGfxContext *ctx, IDXGIFactory4 **factory) {
  D3D12_COMMAND_QUEUE_DESC desc;
  HRESULT made = CreateDXGIFactory2(0, &IID_IDXGIFactory4, (void **)factory);
  if (!osp_gfx_ok(made, "CreateDXGIFactory2")) {
    return 0;
  }
  made = D3D12CreateDevice(NULL, D3D_FEATURE_LEVEL_11_0, &IID_ID3D12Device,
                           (void **)&ctx->device);
  if (!osp_gfx_ok(made, "D3D12CreateDevice")) {
    return 0;
  }
  ZeroMemory(&desc, sizeof desc);
  desc.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
  made = ID3D12Device_CreateCommandQueue(ctx->device, &desc, &IID_ID3D12CommandQueue,
                                         (void **)&ctx->queue);
  return osp_gfx_ok(made, "CreateCommandQueue");
}

// The flip model is the only presentation a modern Windows compositor performs
// without an extra copy; it is also why the buffer count is two rather than one.
int osp_gfx_swapchain(OspGfxContext *ctx, IDXGIFactory4 *factory) {
  DXGI_SWAP_CHAIN_DESC1 desc;
  IDXGISwapChain1 *chain = NULL;
  HRESULT made;
  ZeroMemory(&desc, sizeof desc);
  desc.Width = ctx->width;
  desc.Height = ctx->height;
  desc.Format = OSP_GFX_FORMAT;
  desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
  desc.BufferCount = OSP_GFX_FRAMES;
  desc.SwapEffect = DXGI_SWAP_EFFECT_FLIP_DISCARD;
  desc.SampleDesc.Count = 1;
  made = IDXGIFactory4_CreateSwapChainForHwnd(factory, (IUnknown *)ctx->queue, ctx->window,
                                              &desc, NULL, NULL, &chain);
  if (!osp_gfx_ok(made, "CreateSwapChainForHwnd")) {
    return 0;
  }
  made = IDXGISwapChain1_QueryInterface(chain, &IID_IDXGISwapChain3, (void **)&ctx->swap);
  osp_gfx_release(chain);
  return osp_gfx_ok(made, "IDXGISwapChain3");
}

static int osp_gfx_rtv_heap(OspGfxContext *ctx) {
  D3D12_DESCRIPTOR_HEAP_DESC desc;
  ZeroMemory(&desc, sizeof desc);
  desc.Type = D3D12_DESCRIPTOR_HEAP_TYPE_RTV;
  desc.NumDescriptors = OSP_GFX_FRAMES;
  return osp_gfx_ok(ID3D12Device_CreateDescriptorHeap(ctx->device, &desc,
                                                      &IID_ID3D12DescriptorHeap,
                                                      (void **)&ctx->rtvHeap),
                    "CreateDescriptorHeap");
}

static int osp_gfx_bind_target(OspGfxContext *ctx, UINT index,
                               D3D12_CPU_DESCRIPTOR_HANDLE rtv) {
  HRESULT got = IDXGISwapChain3_GetBuffer(ctx->swap, index, &IID_ID3D12Resource,
                                          (void **)&ctx->targets[index]);
  if (!osp_gfx_ok(got, "GetBuffer")) {
    return 0;
  }
  ctx->rtvs[index] = rtv;
  ID3D12Device_CreateRenderTargetView(ctx->device, ctx->targets[index], NULL, rtv);
  return 1;
}

// One render-target view per back buffer, cached so the frame path never asks
// the heap for its start again. That call is the one place the C bindings for
// D3D12 historically diverge between toolchains; a D3D12_CPU_DESCRIPTOR_HANDLE
// is a single pointer-sized field, so it rides back in a register under the x64
// ABI and MSVC's and mingw-w64's default bindings both return it by value.
int osp_gfx_targets(OspGfxContext *ctx) {
  D3D12_CPU_DESCRIPTOR_HANDLE rtv;
  UINT step;
  UINT i;
  if (!osp_gfx_rtv_heap(ctx)) {
    return 0;
  }
  step = ID3D12Device_GetDescriptorHandleIncrementSize(ctx->device,
                                                       D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
  rtv = ID3D12DescriptorHeap_GetCPUDescriptorHandleForHeapStart(ctx->rtvHeap);
  for (i = 0; i < OSP_GFX_FRAMES; i++, rtv.ptr += (SIZE_T)step) {
    if (!osp_gfx_bind_target(ctx, i, rtv)) {
      return 0;
    }
  }
  return 1;
}

// The Osprey scene names one shader for every platform — base/base.osp says
// "examples/graphics/base.metal", and says it on Windows too, which is the
// whole point of the arrangement. Each backend resolves that name into its own
// dialect by swapping the extension, so a scene never learns which GPU API is
// underneath it. The extension is taken from the file name only, so a relative
// path whose directories contain dots resolves the way a reader expects.
static int osp_gfx_shader_path(const char *requested, char *out, size_t cap) {
  const char *name = requested ? requested : "";
  const char *last = strrchr(name, '/');
  const char *back = strrchr(name, '\\');
  const char *dot;
  size_t stem;
  if (back && (!last || back > last)) {
    last = back;
  }
  dot = strrchr(last ? last : name, '.');
  stem = dot ? (size_t)(dot - name) : strlen(name);
  if (stem == 0 || stem + sizeof OSP_GFX_SHADER_EXT > cap) {
    fprintf(stderr, "ospgfx: unusable shader path %s\n", requested ? requested : "(null)");
    return 0;
  }
  memcpy(out, name, stem);
  memcpy(out + stem, OSP_GFX_SHADER_EXT, sizeof OSP_GFX_SHADER_EXT);
  return 1;
}

static char *osp_gfx_read(const char *path) {
  long size = 0;
  char *text = NULL;
  FILE *file = fopen(path, "rb");
  if (!file) {
    fprintf(stderr, "ospgfx: cannot read shader %s\n", path);
    return NULL;
  }
  if (fseek(file, 0, SEEK_END) == 0 && (size = ftell(file)) > 0 &&
      fseek(file, 0, SEEK_SET) == 0 && (text = (char *)malloc((size_t)size + 1)) != NULL) {
    text[fread(text, 1, (size_t)size, file)] = '\0';
  }
  (void)fclose(file);
  if (!text) {
    fprintf(stderr, "ospgfx: shader %s is empty or unreadable\n", path);
  }
  return text;
}

// Compile one named entry out of the shared library, exactly as the Metal
// bridge pulls one function out of `newLibraryWithSource:`: several scenes
// share one file and differ only in which fragment they name.
static ID3DBlob *osp_gfx_compile(const char *source, const char *path, const char *entry,
                                 const char *target) {
  ID3DBlob *code = NULL;
  ID3DBlob *errors = NULL;
  HRESULT built = D3DCompile(source, strlen(source), path, NULL, NULL, entry, target,
                             OSP_GFX_COMPILE_FLAGS, 0, &code, &errors);
  osp_gfx_report(errors, entry);
  if (osp_gfx_ok(built, "D3DCompile")) {
    return code;
  }
  osp_gfx_release(code);
  return NULL;
}

static int osp_gfx_shaders(const char *shaderPath, const char *entry, ID3DBlob **vs,
                           ID3DBlob **ps) {
  char resolved[OSP_GFX_PATH_MAX];
  char *source;
  if (!osp_gfx_shader_path(shaderPath, resolved, sizeof resolved)) {
    return 0;
  }
  source = osp_gfx_read(resolved);
  if (!source) {
    return 0;
  }
  *vs = osp_gfx_compile(source, resolved, OSP_GFX_VERTEX_ENTRY, OSP_GFX_VERTEX_TARGET);
  *ps = *vs ? osp_gfx_compile(source, resolved, entry, OSP_GFX_FRAGMENT_TARGET) : NULL;
  free(source);
  if (*vs && *ps) {
    return 1;
  }
  fprintf(stderr, "ospgfx: shader needs osp_vertex and %s\n", entry);
  return 0;
}

// One root parameter holding the whole uniform block. Only the pixel stage
// reads it; the vertex stage builds its triangle from SV_VertexID alone.
static ID3DBlob *osp_gfx_root_blob(void) {
  D3D12_ROOT_PARAMETER param;
  D3D12_ROOT_SIGNATURE_DESC desc;
  ID3DBlob *blob = NULL;
  ID3DBlob *errors = NULL;
  HRESULT built;
  ZeroMemory(&param, sizeof param);
  ZeroMemory(&desc, sizeof desc);
  param.ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
  param.Constants.Num32BitValues = OSP_GFX_ROOT_CONSTANTS;
  param.ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
  desc.NumParameters = 1;
  desc.pParameters = &param;
  built = D3D12SerializeRootSignature(&desc, D3D_ROOT_SIGNATURE_VERSION_1, &blob, &errors);
  osp_gfx_report(errors, "root signature");
  return osp_gfx_ok(built, "D3D12SerializeRootSignature") ? blob : NULL;
}

static int osp_gfx_root(OspGfxContext *ctx) {
  ID3DBlob *blob = osp_gfx_root_blob();
  HRESULT made;
  if (!blob) {
    return 0;
  }
  made = ID3D12Device_CreateRootSignature(ctx->device, 0, ID3D10Blob_GetBufferPointer(blob),
                                          ID3D10Blob_GetBufferSize(blob),
                                          &IID_ID3D12RootSignature, (void **)&ctx->root);
  osp_gfx_release(blob);
  return osp_gfx_ok(made, "CreateRootSignature");
}

// No input layout, no depth buffer, no blending and no culling: the pipeline is
// a full-screen triangle and a fragment entry, which is the entire renderer.
// Culling off is what makes the shared vertex stage's winding a non-question
// between the two rasterisers.
static D3D12_GRAPHICS_PIPELINE_STATE_DESC osp_gfx_state_desc(ID3D12RootSignature *root,
                                                             ID3DBlob *vs, ID3DBlob *ps) {
  D3D12_GRAPHICS_PIPELINE_STATE_DESC desc;
  ZeroMemory(&desc, sizeof desc);
  desc.pRootSignature = root;
  desc.VS.pShaderBytecode = ID3D10Blob_GetBufferPointer(vs);
  desc.VS.BytecodeLength = ID3D10Blob_GetBufferSize(vs);
  desc.PS.pShaderBytecode = ID3D10Blob_GetBufferPointer(ps);
  desc.PS.BytecodeLength = ID3D10Blob_GetBufferSize(ps);
  desc.BlendState.RenderTarget[0].RenderTargetWriteMask = D3D12_COLOR_WRITE_ENABLE_ALL;
  desc.SampleMask = UINT_MAX;
  desc.RasterizerState.FillMode = D3D12_FILL_MODE_SOLID;
  desc.RasterizerState.CullMode = D3D12_CULL_MODE_NONE;
  desc.RasterizerState.DepthClipEnable = TRUE;
  desc.PrimitiveTopologyType = D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE;
  desc.NumRenderTargets = 1;
  desc.RTVFormats[0] = OSP_GFX_FORMAT;
  desc.SampleDesc.Count = 1;
  return desc;
}

static int osp_gfx_state(OspGfxContext *ctx, ID3DBlob *vs, ID3DBlob *ps) {
  D3D12_GRAPHICS_PIPELINE_STATE_DESC desc = osp_gfx_state_desc(ctx->root, vs, ps);
  return osp_gfx_ok(ID3D12Device_CreateGraphicsPipelineState(ctx->device, &desc,
                                                             &IID_ID3D12PipelineState,
                                                             (void **)&ctx->pipeline),
                    "CreateGraphicsPipelineState");
}

int osp_gfx_pipeline(OspGfxContext *ctx, const char *shaderPath, const char *fragmentName) {
  const char *entry =
      fragmentName && *fragmentName ? fragmentName : OSP_GFX_DEFAULT_FRAGMENT;
  ID3DBlob *vs = NULL;
  ID3DBlob *ps = NULL;
  int ready = osp_gfx_shaders(shaderPath, entry, &vs, &ps) && osp_gfx_root(ctx) &&
              osp_gfx_state(ctx, vs, ps);
  osp_gfx_release(vs);
  osp_gfx_release(ps);
  return ready;
}

// A command list is born open and the frame path resets it, so it is closed
// once here; otherwise the very first Reset is a validation error.
static int osp_gfx_command_objects(OspGfxContext *ctx) {
  HRESULT made = ID3D12Device_CreateCommandAllocator(ctx->device,
                                                     D3D12_COMMAND_LIST_TYPE_DIRECT,
                                                     &IID_ID3D12CommandAllocator,
                                                     (void **)&ctx->allocator);
  if (!osp_gfx_ok(made, "CreateCommandAllocator")) {
    return 0;
  }
  made = ID3D12Device_CreateCommandList(ctx->device, 0, D3D12_COMMAND_LIST_TYPE_DIRECT,
                                        ctx->allocator, ctx->pipeline,
                                        &IID_ID3D12GraphicsCommandList,
                                        (void **)&ctx->commands);
  if (!osp_gfx_ok(made, "CreateCommandList")) {
    return 0;
  }
  return osp_gfx_ok(ID3D12GraphicsCommandList_Close(ctx->commands), "initial Close");
}

int osp_gfx_frame_objects(OspGfxContext *ctx) {
  HRESULT made;
  if (!osp_gfx_command_objects(ctx)) {
    return 0;
  }
  made = ID3D12Device_CreateFence(ctx->device, 0, D3D12_FENCE_FLAG_NONE, &IID_ID3D12Fence,
                                  (void **)&ctx->fence);
  if (!osp_gfx_ok(made, "CreateFence")) {
    return 0;
  }
  ctx->fenceEvent = CreateEventW(NULL, FALSE, FALSE, NULL);
  if (!ctx->fenceEvent) {
    fprintf(stderr, "ospgfx: cannot create fence event\n");
  }
  return ctx->fenceEvent != NULL;
}

static void osp_gfx_release_gpu(OspGfxContext *ctx) {
  UINT i;
  for (i = 0; i < OSP_GFX_FRAMES; i++) {
    osp_gfx_release(ctx->targets[i]);
  }
  osp_gfx_release(ctx->commands);
  osp_gfx_release(ctx->allocator);
  osp_gfx_release(ctx->pipeline);
  osp_gfx_release(ctx->root);
  osp_gfx_release(ctx->fence);
  osp_gfx_release(ctx->rtvHeap);
  osp_gfx_release(ctx->swap);
  osp_gfx_release(ctx->queue);
  osp_gfx_release(ctx->device);
}

void osp_gfx_destroy(OspGfxContext *ctx) {
  osp_gfx_release_gpu(ctx);
  if (ctx->fenceEvent) {
    (void)CloseHandle(ctx->fenceEvent);
  }
  if (ctx->window) {
    (void)DestroyWindow(ctx->window);
  }
  free(ctx);
}
