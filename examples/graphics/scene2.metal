// The fragment shader the Osprey graphics bridge runs on the Apple GPU. It is
// loaded from disk at `osp_gfx_open` time, so editing this file only requires
// restarting `./target/release/osprey examples/graphics/scene.osp --run`; the
// graphics bridge does not need to be rebuilt.
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

// A compact ACES approximation keeps bright filaments pearlescent instead of
// clipping each channel into harsh neon.
static float3 tonemap(float3 x) {
    return clamp((x * (2.51 * x + 0.03)) /
                 (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

// The Kali fold: `z = |z| / dot(z, z) - c` iterated. The reciprocal turns the
// plane inside out around the origin every step, and the absolute value folds
// it into a quadrant, so a handful of iterations weaves the filament structure
// this scene is made of. `c` is the shape, and Osprey moves it.
fragment float4 osp_fragment(float4 pos [[position]],
                             constant Uniforms &u [[buffer(0)]]) {
    float2 uv = (pos.xy * 2.0 - u.res) / u.res.y;

    // Midnight ink, lifted by two broad pools of indigo light. Giving the
    // voids a deliberate atmosphere lets them work as negative space.
    const float3 ink = float3(0.005, 0.008, 0.025);
    const float3 deepPlum = float3(0.018, 0.008, 0.042);
    const float3 violet = float3(0.40, 0.045, 0.64);
    const float3 cyan = float3(0.018, 0.56, 0.72);
    const float3 pearl = float3(0.68, 0.91, 1.00);
    const float3 coral = float3(0.82, 0.12, 0.28);

    float vertical = saturate(0.52 - 0.20 * uv.y);
    float2 coolPoint = (uv - float2(0.52, -0.30)) * float2(0.78, 1.0);
    float2 warmPoint = (uv - float2(-0.72, 0.45)) * float2(0.72, 1.0);
    float coolWash = exp(-2.70 * dot(coolPoint, coolPoint));
    float warmWash = exp(-3.20 * dot(warmPoint, warmPoint));
    float3 col = mix(ink, deepPlum, 0.12 + 0.12 * vertical + 0.18 * warmWash);
    col += float3(0.005, 0.014, 0.024) * coolWash;

    float a = u.slot[3] * 6.2831853;
    float2x2 rot = float2x2(cos(a), sin(a), -sin(a), cos(a));
    float2 z = rot * uv * u.slot[2] + float2(u.slot[6], u.slot[7]);
    float2 c = float2(u.slot[0], u.slot[1]);

    // The orbit contributes bounded scalar fields instead of raw RGB. This
    // separates the broad woven haze, readable filaments, and rare bright core.
    float energy = 0.0;
    float cyanEnergy = 0.0;
    float trap = 1e9;
    for (int i = 0; i < 13; i++) {
        z = abs(z) / max(dot(z, z), 1e-5) - c;
        float d = min(length(z), 16.0);
        trap = min(trap, d);

        float e = exp(-4.0 * d) * (1.0 - 0.025 * float(i));
        float flow = 0.5 + 0.5 * cos(6.2831853 *
            (0.071 * float(i) + 0.11 * d + u.slot[4]));
        energy += e;
        cyanEnergy += e * flow;
    }

    float flowMix = cyanEnergy / max(energy, 1e-4);
    float3 silk = mix(violet, cyan, smoothstep(0.32, 0.68, flowMix));
    float haze = 1.0 - exp(-0.08 * energy);
    float filament = 1.0 - exp(-0.40 * energy);
    float halo = exp(-14.0 * trap);
    float core = exp(-800.0 * trap * trap);

    float2 focusPoint = (uv - float2(-0.10, 0.04)) * float2(0.62, 1.0);
    float focus = 0.54 + 0.58 * exp(-0.95 * dot(focusPoint, focusPoint));

    col += silk * (0.014 * haze * focus);
    col += silk * (0.040 * filament + 0.10 * halo) * focus;
    col += pearl * core * (0.018 + 0.035 * u.slot[5]);

    // One very quiet warm glint keeps the duotone from feeling synthetic while
    // preserving the cyan-violet identity of the piece.
    float focal = exp(-1.35 * dot(warmPoint, warmPoint));
    col += coral * (0.024 * focal * core * core);

    // Elliptical edge falloff respects the wide canvas and never clips through
    // black at the corners as the old quadratic vignette did.
    float edge = length(uv * float2(0.62, 1.0));
    col *= 1.0 - 0.38 * smoothstep(0.42, 1.48, edge);

    col = tonemap(col * 1.08);
    return float4(pow(max(col, 0.0), float3(1.0 / 2.2)), 1.0);
}
