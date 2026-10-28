// Osprey -> Windows graphics bridge: a Win32 window backed by a Direct3D 12
// swap chain, exposing the same flat C ABI the Swift host (macos/) defines on macOS
// ([FFI-LINK-DIRECTIVES], [FFI-PTR] — docs/specs/compiler/0019-ForeignFunctionInterface.md).
//
// VERIFIED ON WINDOWS ON 2026-10-28, AS FAR AS THIS GOES. Authored on macOS
// from the Metal bridge and the Direct3D 12 documentation, it now builds with
// MSYS2 UCRT64's gcc and -Wall -Werror, and on GitHub's windows-latest runner the
// unchanged scenes link against it and draw in real windows: 10 frames each of
// Kali, Opal and Character, and a picture of each window after twenty seconds.
// That runner has no GPU, so Direct3D 12 drew on the Microsoft Basic Render
// Driver, in software, and no frame has been compared with the Mac's at matched
// uniforms, the only check of the cbuffer packing and the matrix order. The first compile's three mistakes and their
// fixes are in README.md under "Verification status", with the commands.
//
// macos/GraphicsBridge.swift is the reference. It carries the rationale — Osprey
// as the host that owns the window, the clock and the scene state; the GPU as
// the rasteriser; the two slot scales; why the fragment entry is a parameter —
// and this backend does not repeat that prose, recording only what Direct3D
// does differently. The six exported symbols, their semantics and the
// fixed-point encoding are identical, which is why base/base.osp is
// byte-identical across platforms and never learns which API is underneath it.
//
// WHAT DIRECT3D DOES DIFFERENTLY.
//   * The uniforms travel as root constants rather than `setFragmentBytes:`, so
//     there is no upload heap and no per-frame resource to manage.
//   * Every allocation returns an HRESULT instead of a nil object, so failure
//     is checked through `osp_gfx_ok`, which names the call and its code.
//   * The window is a raw Win32 class, and the closed flag is raised from the
//     window procedure rather than from an NSWindow delegate.
//   * Presentation is explicit: record, close, execute, present, wait on a
//     fence. `nextDrawable` did that throttling implicitly on macOS.
//
// THE SPLIT. Bringing up a D3D12 device is several times the code Metal needs
// for the same picture, so the backend is three translation units around this
// header rather than one file over the repository's length limit:
// ospgfx_d3d12_window.c makes the Win32 window, ospgfx_d3d12_setup.c builds
// everything that draws into it once, and ospgfx_d3d12.c is the C ABI and the
// per-frame path. Nothing here is part of the Osprey-facing contract — only the
// six `OSP_GFX_API` functions in ospgfx_d3d12.c are.

#ifndef OSPGFX_D3D12_H
#define OSPGFX_D3D12_H

#define _CRT_SECURE_NO_WARNINGS
#define COBJMACROS
#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <d3d12.h>
#include <d3dcompiler.h>
#include <dxgi1_4.h>
#include <stdint.h>

// The slot count, the two fixed-point scales, the uniform block and the
// argument validation are the Osprey-facing ABI shared with the Swift host, declared
// once in ospgfx_abi.h. base.hlsl mirrors the uniform layout by hand — the GPU
// cannot see the header — and crates/osprey-cli/tests/graphics_scenes.rs
// asserts the mirror agrees.
#include "ospgfx_abi.h"

// Naming exactly the symbols Osprey's `extern fn` declarations look for is what
// MSVC needs to export anything at all, and what stops mingw-w64 exporting
// every other function in the two translation units as well.
#define OSP_GFX_API __declspec(dllexport)

// The whole uniform block travels as root constants, one DWORD per float.
#define OSP_GFX_ROOT_CONSTANTS ((UINT)(sizeof(OspGfxUniforms) / sizeof(float)))
// A root signature is 64 DWORDs in total. Twenty-four slots plus res, time and
// pad is 28, so the block fits with room to spare — but growing OSP_GFX_SLOTS
// past that budget stops being free and fails at pipeline creation with an
// opaque validation message. This array fails to compile instead.
#define OSP_GFX_ROOT_BUDGET 64
typedef char osp_gfx_root_constants_fit[OSP_GFX_ROOT_CONSTANTS <= OSP_GFX_ROOT_BUDGET ? 1 : -1];

// Double buffered, the minimum a flip-model swap chain accepts.
#define OSP_GFX_FRAMES 2u
#define OSP_GFX_FORMAT DXGI_FORMAT_B8G8R8A8_UNORM
#define OSP_GFX_TRIANGLE_VERTICES 3u
#define OSP_GFX_VSYNC_INTERVAL 1u
// Long enough that a heavily loaded GPU is never mistaken for a hung one, short
// enough that a genuinely hung device does not wedge the host forever.
#define OSP_GFX_FRAME_TIMEOUT_MS 5000u
#define OSP_GFX_TITLE_MAX 256
#define OSP_GFX_PATH_MAX 1024
#define OSP_GFX_DPI_DEFAULT 96u

// Titled, closable and minimisable but not resizable — the same window the
// Metal bridge asks Cocoa for, and the reason no swap-chain resize path exists.
#define OSP_GFX_WINDOW_STYLE (WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX)

// Shader Model 5.0 through the legacy fxc-compatible `D3DCompile`. The current
// Microsoft compiler is DXC (LLVM-based, required for Shader Model 6); 5.0 is
// kept because the library needs nothing from SM6 and fxc is what every SDK
// still ships on PATH-less installs.
#define OSP_GFX_SHADER_EXT ".hlsl"
#define OSP_GFX_VERTEX_TARGET "vs_5_0"
#define OSP_GFX_FRAGMENT_TARGET "ps_5_0"
#define OSP_GFX_COMPILE_FLAGS D3DCOMPILE_OPTIMIZATION_LEVEL3

typedef struct OspGfxContext {
  HWND window;
  ID3D12Device *device;
  ID3D12CommandQueue *queue;
  IDXGISwapChain3 *swap;
  ID3D12DescriptorHeap *rtvHeap;
  ID3D12Resource *targets[OSP_GFX_FRAMES];
  D3D12_CPU_DESCRIPTOR_HANDLE rtvs[OSP_GFX_FRAMES];
  ID3D12CommandAllocator *allocator;
  ID3D12GraphicsCommandList *commands;
  ID3D12RootSignature *root;
  ID3D12PipelineState *pipeline;
  ID3D12Fence *fence;
  HANDLE fenceEvent;
  UINT64 fenceValue;
  UINT width;
  UINT height;
  int closed;
  double openedAt;
  OspGfxUniforms uniforms;
  // OSPREY_GFX_FRAMES ends a scene after that many frames, as if its window closed; 0 never does.
  long long frameLimit;
  long long frames;
} OspGfxContext;

// Seconds from the performance counter, which is monotonic, so a paused or
// reset wall clock cannot make an animation jump backwards.
double osp_gfx_now(void);

// Every Direct3D call is checked through here, so a failure is one diagnostic
// naming the call and its code rather than a blank window that reads as a
// renderer bug. Returns 1 on success, so calls chain with `&&`.
int osp_gfx_ok(HRESULT hr, const char *what);

// Release every COM object through one helper, so a context that failed halfway
// through `osp_gfx_open` tears down by exactly the path a whole one does.
void osp_gfx_release(void *object);

// The one-time bring-up, in the order osp_gfx_open runs it. Each returns 1 on
// success and has already reported the reason on failure.
int osp_gfx_make_window(OspGfxContext *ctx, int64_t width, int64_t height,
                        const char *title);
int osp_gfx_device(OspGfxContext *ctx, IDXGIFactory4 **factory);
int osp_gfx_swapchain(OspGfxContext *ctx, IDXGIFactory4 *factory);
int osp_gfx_targets(OspGfxContext *ctx);
int osp_gfx_pipeline(OspGfxContext *ctx, const char *shaderPath, const char *fragmentName);
int osp_gfx_frame_objects(OspGfxContext *ctx);
void osp_gfx_destroy(OspGfxContext *ctx);

#endif
