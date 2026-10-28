// The Win32 window the Direct3D 12 backend draws into: its class, its procedure,
// its DPI awareness and its title. `osp_gfx_open` makes it first, through
// osp_gfx_make_window; ospgfx_d3d12_setup.c brings up everything that draws into
// it, and ospgfx_d3d12.c pumps its messages between frames. Plan 0040 moves the
// scenes into a WinUI panel, and this file is what that change deletes.

#include "ospgfx_d3d12.h"

#include <stdio.h>

// DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, spelled as its numeric value so
// this file still builds against an SDK too old to declare the type.
#define OSP_GFX_DPI_PER_MONITOR_V2 ((HANDLE)(INT_PTR)-4)

static const wchar_t OSP_GFX_CLASS[] = L"OspreyGfxWindow";

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
  // IDC_ARROW is an integer resource id spelled as a narrow string unless UNICODE is defined.
  cls.hCursor = LoadCursorW(NULL, (LPCWSTR)IDC_ARROW);
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
