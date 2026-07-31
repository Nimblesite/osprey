// Osprey -> macOS graphics bridge: a Cocoa window backed by a Metal layer,
// exposed as a flat C ABI for Osprey's FFI ([FFI-LINK-DIRECTIVES], [FFI-PTR] —
// docs/specs/0019-ForeignFunctionInterface.md).
//
// The division of labour is the same one every real-time renderer uses. Osprey
// is the host: it owns the window, the clock, and the scene state it recomputes
// every frame. The GPU is the rasteriser: a fragment shader loaded from disk
// shades every pixel from the parameter slots Osprey pushes into it.
//
// Osprey's own `gpu*` builtins are NOT this path — [GPU-BACKEND-HOST] is the
// only backend implemented, so they run on the CPU. This bridge is the seam
// where [GPU-BACKEND-DEVICE] would plug in once it exists.

#import <Cocoa/Cocoa.h>
#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>
#include <stdint.h>

// Parameter slots Osprey can push per frame, in fixed point (4096 = 1.0).
#define OSP_GFX_SLOTS 16
#define OSP_GFX_FIXED_ONE 4096.0f

typedef struct OspGfxUniforms {
  float slot[OSP_GFX_SLOTS];
  float res[2];
  float time;
  float pad;
} OspGfxUniforms;

@interface OspGfxContext : NSObject <NSWindowDelegate>
@property(nonatomic, strong) NSWindow *window;
@property(nonatomic, strong) CAMetalLayer *layer;
@property(nonatomic, strong) id<MTLCommandQueue> queue;
@property(nonatomic, strong) id<MTLRenderPipelineState> pipeline;
@property(nonatomic) BOOL closed;
@property(nonatomic) double openedAt;
@property(nonatomic) OspGfxUniforms uniforms;
@end

@implementation OspGfxContext
- (BOOL)windowShouldClose:(NSWindow *)sender {
  (void)sender;
  self.closed = YES;
  return YES;
}
@end

// Seconds from a monotonic clock, so a paused or reset wall clock cannot make
// an animation jump backwards.
static double osp_gfx_now(void) {
  return (double)clock_gettime_nsec_np(CLOCK_MONOTONIC) / 1e9;
}

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

// Compile the fragment shader Osprey named. A shader that will not compile is
// reported with its diagnostics and fails the open, rather than presenting a
// blank window that looks like a renderer bug.
static id<MTLRenderPipelineState> osp_gfx_pipeline(id<MTLDevice> device,
                                                   const char *shaderPath) {
  NSError *error = nil;
  NSString *path = [NSString stringWithUTF8String:shaderPath ? shaderPath : ""];
  NSString *source = [NSString stringWithContentsOfFile:path
                                               encoding:NSUTF8StringEncoding
                                                  error:&error];
  if (!source) {
    fprintf(stderr, "ospgfx: cannot read shader %s\n", shaderPath);
    return nil;
  }
  id<MTLLibrary> library = [device newLibraryWithSource:source
                                                options:nil
                                                  error:&error];
  if (!library) {
    fprintf(stderr, "ospgfx: shader failed to compile:\n%s\n",
            [[error localizedDescription] UTF8String]);
    return nil;
  }
  MTLRenderPipelineDescriptor *desc = [[MTLRenderPipelineDescriptor alloc] init];
  desc.vertexFunction = [library newFunctionWithName:@"osp_vertex"];
  desc.fragmentFunction = [library newFunctionWithName:@"osp_fragment"];
  desc.colorAttachments[0].pixelFormat = MTLPixelFormatBGRA8Unorm;
  if (!desc.vertexFunction || !desc.fragmentFunction) {
    fprintf(stderr, "ospgfx: shader needs osp_vertex and osp_fragment\n");
    return nil;
  }
  id<MTLRenderPipelineState> pipeline =
      [device newRenderPipelineStateWithDescriptor:desc error:&error];
  if (!pipeline) {
    fprintf(stderr, "ospgfx: pipeline failed: %s\n",
            [[error localizedDescription] UTF8String]);
  }
  return pipeline;
}

static void osp_gfx_attach_layer(OspGfxContext *ctx, id<MTLDevice> device,
                                 NSRect frame) {
  NSView *view = [[NSView alloc] initWithFrame:frame];
  ctx.layer = [CAMetalLayer layer];
  ctx.layer.device = device;
  ctx.layer.pixelFormat = MTLPixelFormatBGRA8Unorm;
  ctx.layer.framebufferOnly = YES;
  [view setWantsLayer:YES];
  [view setLayer:ctx.layer];
  [ctx.window setContentView:view];
  CGFloat backing = [ctx.window backingScaleFactor];
  ctx.layer.contentsScale = backing;
  ctx.layer.drawableSize =
      CGSizeMake(frame.size.width * backing, frame.size.height * backing);
}

// `scale` magnifies the design resolution into window points: 320x200 at
// scale 3 opens a 960x600 window, drawn at the display's full backing density.
void *osp_gfx_open(int64_t width, int64_t height, int64_t scale,
                   const char *title, const char *shaderPath) {
  if (width <= 0 || height <= 0 || width > 8192 || height > 8192) {
    return NULL;
  }
  if (scale < 1 || scale > 16) {
    scale = 1;
  }
  osp_gfx_boot_app();
  id<MTLDevice> device = MTLCreateSystemDefaultDevice();
  if (!device) {
    fprintf(stderr, "ospgfx: no Metal device\n");
    return NULL;
  }
  id<MTLRenderPipelineState> pipeline = osp_gfx_pipeline(device, shaderPath);
  if (!pipeline) {
    return NULL;
  }
  OspGfxContext *ctx = [[OspGfxContext alloc] init];
  ctx.openedAt = osp_gfx_now();
  ctx.queue = [device newCommandQueue];
  ctx.pipeline = pipeline;
  NSRect frame = NSMakeRect(0, 0, (CGFloat)(width * scale),
                            (CGFloat)(height * scale));
  ctx.window = [[NSWindow alloc]
      initWithContentRect:frame
                styleMask:(NSWindowStyleMaskTitled | NSWindowStyleMaskClosable |
                           NSWindowStyleMaskMiniaturizable)
                  backing:NSBackingStoreBuffered
                    defer:NO];
  [ctx.window setTitle:[NSString stringWithUTF8String:title ? title : "Osprey"]];
  [ctx.window setDelegate:ctx];
  osp_gfx_attach_layer(ctx, device, frame);
  [ctx.window center];
  [ctx.window makeKeyAndOrderFront:nil];
  return (__bridge_retained void *)ctx;
}

// Push one scene parameter. Osprey works in fixed point, so `value` is in
// 4096ths; the shader sees a float. Out-of-range slots are ignored rather than
// trapping, so a scene bug never takes the process down.
int64_t osp_gfx_set(void *handle, int64_t slot, int64_t value) {
  OspGfxContext *ctx = (__bridge OspGfxContext *)handle;
  if (!ctx || slot < 0 || slot >= OSP_GFX_SLOTS) {
    return 0;
  }
  OspGfxUniforms u = ctx.uniforms;
  u.slot[slot] = (float)value / OSP_GFX_FIXED_ONE;
  ctx.uniforms = u;
  return 1;
}

static void osp_gfx_pump_events(void) {
  NSEvent *event;
  while ((event = [NSApp nextEventMatchingMask:NSEventMaskAny
                                     untilDate:nil
                                        inMode:NSDefaultRunLoopMode
                                       dequeue:YES])) {
    [NSApp sendEvent:event];
  }
}

// Shade every pixel from the current slots and present. Returns 1 while the
// window is open, 0 once the user has closed it.
int64_t osp_gfx_draw(void *handle) {
  OspGfxContext *ctx = (__bridge OspGfxContext *)handle;
  if (!ctx || ctx.closed) {
    return 0;
  }
  @autoreleasepool {
    id<CAMetalDrawable> drawable = [ctx.layer nextDrawable];
    if (drawable) {
      CGSize size = ctx.layer.drawableSize;
      OspGfxUniforms u = ctx.uniforms;
      u.res[0] = (float)size.width;
      u.res[1] = (float)size.height;
      u.time = (float)(osp_gfx_now() - ctx.openedAt);
      ctx.uniforms = u;
      MTLRenderPassDescriptor *pass = [MTLRenderPassDescriptor renderPassDescriptor];
      pass.colorAttachments[0].texture = drawable.texture;
      pass.colorAttachments[0].loadAction = MTLLoadActionClear;
      pass.colorAttachments[0].storeAction = MTLStoreActionStore;
      pass.colorAttachments[0].clearColor = MTLClearColorMake(0, 0, 0, 1);
      id<MTLCommandBuffer> commands = [ctx.queue commandBuffer];
      id<MTLRenderCommandEncoder> encoder =
          [commands renderCommandEncoderWithDescriptor:pass];
      [encoder setRenderPipelineState:ctx.pipeline];
      [encoder setFragmentBytes:&u length:sizeof(u) atIndex:0];
      [encoder drawPrimitives:MTLPrimitiveTypeTriangle vertexStart:0 vertexCount:3];
      [encoder endEncoding];
      [commands presentDrawable:drawable];
      [commands commit];
    }
    osp_gfx_pump_events();
  }
  return ctx.closed ? 0 : 1;
}

// Milliseconds since the window opened. Animations advance on wall-clock time
// so they play at the same speed whatever frame rate the host sustains.
int64_t osp_gfx_ticks(void *handle) {
  OspGfxContext *ctx = (__bridge OspGfxContext *)handle;
  return ctx ? (int64_t)((osp_gfx_now() - ctx.openedAt) * 1000.0) : 0;
}

int64_t osp_gfx_close(void *handle) {
  OspGfxContext *ctx = (__bridge_transfer OspGfxContext *)handle;
  if (!ctx) {
    return 0;
  }
  [ctx.window setDelegate:nil];
  [ctx.window orderOut:nil];
  return 1;
}
