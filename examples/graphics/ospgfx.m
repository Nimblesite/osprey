// Cocoa window + framebuffer blit, exposed as a flat C ABI for Osprey's FFI
// ([FFI-LINK-DIRECTIVES], [FFI-PTR] — docs/specs/0019-ForeignFunctionInterface.md).
// Osprey renders pixels; this file only owns the window and the presentation.

#import <Cocoa/Cocoa.h>
#include <stdint.h>
#include <stdlib.h>

// One 32-bit BGRA pixel per cell; the CGImage below reads it host-endian.
typedef struct OspGfxWindow {
  int64_t width;
  int64_t height;
  uint32_t *pixels;
  NSWindow *window;
  NSImageView *view;
  int64_t closed;
  double opened_at;
} OspGfxWindow;

@interface OspGfxDelegate : NSObject <NSWindowDelegate>
@property(assign) OspGfxWindow *owner;
@end

@implementation OspGfxDelegate
- (BOOL)windowShouldClose:(NSWindow *)sender {
  (void)sender;
  if (self.owner) {
    self.owner->closed = 1;
  }
  return YES;
}
@end

// Bring up NSApplication once, as a regular foreground app so the window
// actually takes focus when launched from a terminal.
static void osp_gfx_boot_app(void) {
  static int booted = 0;
  if (booted) {
    return;
  }
  booted = 1;
  [NSApplication sharedApplication];
  [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
  [NSApp finishLaunching];
  [NSApp activateIgnoringOtherApps:YES];
}

// Seconds since some fixed origin, monotonic enough to drive an animation.
static double osp_gfx_now(void) {
  return (double)clock_gettime_nsec_np(CLOCK_MONOTONIC) / 1e9;
}

// `scale` magnifies the framebuffer on screen: a 320x200 buffer at scale 3
// fills a 960x600 window. Osprey renders at buffer resolution either way.
void *osp_gfx_open(int64_t width, int64_t height, int64_t scale,
                   const char *title) {
  if (width <= 0 || height <= 0 || width > 8192 || height > 8192) {
    return NULL;
  }
  if (scale < 1 || scale > 16) {
    scale = 1;
  }
  osp_gfx_boot_app();
  OspGfxWindow *w = (OspGfxWindow *)calloc(1, sizeof(OspGfxWindow));
  if (!w) {
    return NULL;
  }
  w->width = width;
  w->height = height;
  w->pixels = (uint32_t *)calloc((size_t)(width * height), sizeof(uint32_t));
  if (!w->pixels) {
    free(w);
    return NULL;
  }
  w->opened_at = osp_gfx_now();
  NSRect frame = NSMakeRect(0, 0, (CGFloat)(width * scale),
                            (CGFloat)(height * scale));
  w->window = [[NSWindow alloc]
      initWithContentRect:frame
                styleMask:(NSWindowStyleMaskTitled | NSWindowStyleMaskClosable |
                           NSWindowStyleMaskMiniaturizable)
                  backing:NSBackingStoreBuffered
                    defer:NO];
  [w->window setTitle:[NSString stringWithUTF8String:title ? title : "Osprey"]];
  OspGfxDelegate *delegate = [[OspGfxDelegate alloc] init];
  delegate.owner = w;
  [w->window setDelegate:delegate];
  w->view = [[NSImageView alloc] initWithFrame:frame];
  [w->view setImageScaling:NSImageScaleAxesIndependently];
  [w->window setContentView:w->view];
  [w->window center];
  [w->window makeKeyAndOrderFront:nil];
  return w;
}

// Write one pixel by flat index. Out-of-range indices are ignored rather than
// trapping, so a renderer bug never takes the process down.
int64_t osp_gfx_put(void *handle, int64_t index, int64_t rgb) {
  OspGfxWindow *w = (OspGfxWindow *)handle;
  if (!w || index < 0 || index >= w->width * w->height) {
    return 0;
  }
  w->pixels[index] = 0xFF000000u | (uint32_t)(rgb & 0xFFFFFF);
  return 1;
}

// Publish the framebuffer and drain pending events. Returns 1 while the
// window is open, 0 once the user has closed it.
int64_t osp_gfx_present(void *handle) {
  OspGfxWindow *w = (OspGfxWindow *)handle;
  if (!w || w->closed) {
    return 0;
  }
  @autoreleasepool {
    CGColorSpaceRef space = CGColorSpaceCreateDeviceRGB();
    CGContextRef ctx = CGBitmapContextCreate(
        w->pixels, (size_t)w->width, (size_t)w->height, 8,
        (size_t)w->width * 4, space,
        kCGImageAlphaNoneSkipFirst | kCGBitmapByteOrder32Little);
    CGImageRef cg = ctx ? CGBitmapContextCreateImage(ctx) : NULL;
    if (cg) {
      NSImage *image = [[NSImage alloc]
          initWithCGImage:cg
                     size:NSMakeSize((CGFloat)w->width, (CGFloat)w->height)];
      [w->view setImage:image];
      CGImageRelease(cg);
    }
    if (ctx) {
      CGContextRelease(ctx);
    }
    CGColorSpaceRelease(space);
    NSEvent *event;
    while ((event = [NSApp nextEventMatchingMask:NSEventMaskAny
                                       untilDate:nil
                                          inMode:NSDefaultRunLoopMode
                                         dequeue:YES])) {
      [NSApp sendEvent:event];
    }
  }
  return w->closed ? 0 : 1;
}

// Milliseconds since the window opened. Animations advance on wall-clock time
// so they play at the same speed whatever frame rate the renderer sustains.
int64_t osp_gfx_ticks(void *handle) {
  OspGfxWindow *w = (OspGfxWindow *)handle;
  return w ? (int64_t)((osp_gfx_now() - w->opened_at) * 1000.0) : 0;
}

// Block until the window is closed, pumping events so it stays responsive.
int64_t osp_gfx_wait(void *handle) {
  OspGfxWindow *w = (OspGfxWindow *)handle;
  if (!w) {
    return 0;
  }
  @autoreleasepool {
    while (!w->closed) {
      NSEvent *event = [NSApp nextEventMatchingMask:NSEventMaskAny
                                          untilDate:[NSDate distantFuture]
                                             inMode:NSDefaultRunLoopMode
                                            dequeue:YES];
      if (event) {
        [NSApp sendEvent:event];
      }
    }
  }
  return 1;
}

int64_t osp_gfx_close(void *handle) {
  OspGfxWindow *w = (OspGfxWindow *)handle;
  if (!w) {
    return 0;
  }
  [w->window orderOut:nil];
  free(w->pixels);
  free(w);
  return 1;
}
