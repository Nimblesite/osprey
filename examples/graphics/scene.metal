// The fragment shader the Osprey graphics bridge runs on the Apple GPU. It is
// loaded from disk at `osp_gfx_open` time, so editing this file and re-running
// `make graphics` is the whole edit loop — no recompile of anything else.
//
// Osprey owns every number that moves. This shader reads them from `slot`,
// which the host fills with `osp_gfx_set` each frame; nothing here animates on
// its own. `time` is available but the demo deliberately leaves it unused, so
// the motion you see is Osprey's arithmetic and not the GPU's.

#include <metal_stdlib>
using namespace metal;

struct Uniforms {
    float slot[16];
    float2 res;
    float time;
    float pad;
};

// A full-screen triangle, built from the vertex id alone — no vertex buffer.
vertex float4 osp_vertex(uint vid [[vertex_id]]) {
    float2 p = float2(float((vid << 1) & 2), float(vid & 2));
    return float4(p * 2.0 - 1.0, 0.0, 1.0);
}

// Iñigo Quílez's cosine palette: three phase-shifted cosines sweeping a smooth
// closed loop through colour space.
static float3 palette(float t) {
    return 0.5 + 0.5 * cos(6.2831853 * (t + float3(0.0, 0.33, 0.67)));
}

// The Kali fold: `z = |z| / dot(z, z) - c` iterated. The reciprocal turns the
// plane inside out around the origin every step, and the absolute value folds
// it into a quadrant, so a handful of iterations weaves the filament structure
// this scene is made of. `c` is the shape, and Osprey moves it.
fragment float4 osp_fragment(float4 pos [[position]],
                             constant Uniforms &u [[buffer(0)]]) {
    float2 uv = (pos.xy * 2.0 - u.res) / u.res.y;
    float a = u.slot[3] * 6.2831853;
    float2x2 rot = float2x2(cos(a), sin(a), -sin(a), cos(a));
    float2 z = rot * uv * u.slot[2] + float2(u.slot[6], u.slot[7]);
    float2 c = float2(u.slot[0], u.slot[1]);
    float3 col = float3(0.0);
    float trap = 1e9;
    for (int i = 0; i < 18; i++) {
        z = abs(z) / max(dot(z, z), 1e-5) - c;
        float d = min(length(z), 24.0);
        trap = min(trap, d);
        // A sharp falloff is what keeps the field black: only the iterations
        // that land near the fold's fixed point contribute any light at all.
        col += palette(0.085 * float(i) + 0.45 * d + u.slot[4]) * exp(-3.6 * d);
    }
    // The orbit trap is how close the pixel ever came to that fixed point over
    // the whole orbit; the reciprocal of it is the neon core of each filament.
    col = col * 0.42 + u.slot[5] * palette(u.slot[4] + 0.5) * 0.09 /
          (0.006 + 3.0 * trap * trap);
    col = mix(float3(dot(col, float3(0.299, 0.587, 0.114))), col, 1.45);
    col *= 1.0 - 0.30 * dot(uv, uv);                    // vignette
    col = col / (1.0 + col);                            // reinhard tonemap
    return float4(pow(max(col, 0.0), float3(0.72)), 1.0);
}
