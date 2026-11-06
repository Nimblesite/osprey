// Standalone Metal Q4_K matvec on the probe fixture: one thread per row, 32-bit
// indices, uint loads; N dispatches in one command buffer. The attainable speed
// for a library kernel [LLM-MATMUL-QUANT], against the compiled-kernel probe.
#import <Metal/Metal.h>
#import <Foundation/Foundation.h>
#include <mach/mach_time.h>
static NSString *src = @R"(
#include <metal_stdlib>
using namespace metal;
inline void scale_min(const device uchar *q, uint j, thread uint &sc, thread uint &m) {
    if (j < 4) { sc = q[j] & 63; m = q[j + 4] & 63; }
    else { sc = (q[j + 4] & 0xF) | ((q[j - 4] >> 6) << 4); m = (q[j + 4] >> 4) | ((q[j] >> 6) << 4); }
}
kernel void q4k_matvec(const device uchar *w [[buffer(0)]], const device float *x [[buffer(1)]],
                       device float *out [[buffer(2)]], constant uint &nb [[buffer(3)]],
                       uint r [[thread_position_in_grid]]) {
    float acc = 0.0f;
    for (uint blk = 0; blk < nb; blk++) {
        const device uchar *b = w + (r * nb + blk) * 144;
        float d = float(*(const device half *)(b));
        float dmin = float(*(const device half *)(b + 2));
        const device uchar *scales = b + 4;
        const device uchar *qs = b + 16;
        const device float *xb = x + blk * 256;
        for (uint j = 0; j < 8; j++) {
            uint sc, m; scale_min(scales, j, sc, m);
            float d1 = d * float(sc), m1 = dmin * float(m);
            uint g = j >> 1, high = j & 1;
            const device uchar *q = qs + g * 32;
            const device float *xe = xb + g * 64 + high * 32;
            for (uint l = 0; l < 32; l++) {
                uint nib = high ? (q[l] >> 4) : (q[l] & 15);
                acc = acc + xe[l] * (d1 * float(nib) - m1);
            }
        }
    }
    out[r] = acc;
}
)";
int main(int argc, char **argv) {
    int n = argc > 1 ? atoi(argv[1]) : 400;
    @autoreleasepool {
        id<MTLDevice> dev = MTLCreateSystemDefaultDevice();
        NSError *err = nil;
        MTLCompileOptions *opt = [MTLCompileOptions new];
        opt.mathMode = MTLMathModeSafe;
        id<MTLLibrary> lib = [dev newLibraryWithSource:src options:opt error:&err];
        if (!lib) { NSLog(@"%@", err); return 1; }
        id<MTLComputePipelineState> pso = [dev newComputePipelineStateWithFunction:[lib newFunctionWithName:@"q4k_matvec"] error:&err];
        NSData *wd = [NSData dataWithContentsOfFile:@"q4k.bin"], *xd = [NSData dataWithContentsOfFile:@"x.bin"];
        id<MTLBuffer> w = [dev newBufferWithBytes:wd.bytes length:wd.length options:MTLResourceStorageModeShared];
        id<MTLBuffer> x = [dev newBufferWithBytes:xd.bytes length:xd.length options:MTLResourceStorageModeShared];
        id<MTLBuffer> out = [dev newBufferWithLength:17408 * 4 options:MTLResourceStorageModeShared];
        uint nb = 20;
        id<MTLCommandQueue> q = [dev newCommandQueue];
        for (int pass = 0; pass < 2; pass++) {
            uint64_t t0 = mach_absolute_time();
            id<MTLCommandBuffer> cb = [q commandBuffer];
            id<MTLComputeCommandEncoder> enc = [cb computeCommandEncoder];
            [enc setComputePipelineState:pso];
            [enc setBuffer:w offset:0 atIndex:0]; [enc setBuffer:x offset:0 atIndex:1]; [enc setBuffer:out offset:0 atIndex:2];
            [enc setBytes:&nb length:4 atIndex:3];
            for (int i = 0; i < (pass ? n : 1); i++) [enc dispatchThreads:MTLSizeMake(17408, 1, 1) threadsPerThreadgroup:MTLSizeMake(256, 1, 1)];
            [enc endEncoding]; [cb commit]; [cb waitUntilCompleted];
            uint64_t t1 = mach_absolute_time();
            mach_timebase_info_data_t tb; mach_timebase_info(&tb);
            double ms = (double)(t1 - t0) * tb.numer / tb.denom / 1e6;
            float *o = out.contents;
            printf("%s: %d dispatches %.1f ms, %.3f ms each; out[0]=%.6f out[1]=%.6f out[17407]=%.6f\n", pass ? "timed" : "warm", pass ? n : 1, ms, ms / (pass ? n : 1), o[0], o[1], o[17407]);
        }
    }
    return 0;
}
