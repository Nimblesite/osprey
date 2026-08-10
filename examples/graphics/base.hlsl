// The Osprey HLSL library: the Direct3D 12 twin of base.metal. One file, one
// uniform layout, one full-screen vertex stage, one shared helper layer, and
// the same three named fragment entries. `osp_gfx_open` in ospgfx_d3d12.c
// compiles this file with D3DCompile at run time and picks a fragment by name,
// exactly as the Metal bridge does with `newLibraryWithSource:`. A scene is a
// name plus a table of numbers on both platforms, which is why the Osprey
// sources under base/ are byte-identical across them.
//
// UNVERIFIED ON THE MACHINE THAT WROTE IT. Authored on macOS, which has no fxc,
// no dxc and no Windows SDK, so this file has never been through a shader
// compiler. `make graphics-shader` on Windows compiles all four entry points
// and is what turns careful transcription into a fact; README.md in this
// directory lists the exact commands.
//
// base.metal is the reference. It carries the rationale for every constant,
// every helper and where the dividing line between Osprey and the GPU falls,
// and this file deliberately does not repeat that prose — what is written below
// is only what HLSL does differently. Keeping the two structurally parallel,
// section for section and helper for helper, is what makes a change to one
// obviously a change to the other; the two are pinned together by
// crates/osprey-cli/tests/graphics_scenes.rs, which requires every constant
// base.metal declares to exist here too.
//
// WHAT HLSL DOES DIFFERENTLY.
//   * Constant-buffer packing, the one difference that can corrupt an image in
//     silence. A `float slot[24]` declared inside a cbuffer does NOT pack
//     tightly: HLSL gives every array element its own 16-byte register, so it
//     would span 384 bytes and read `slot[1]` out of the bridge's `slot[4]`.
//     Nothing about that fails to compile. Declaring the array as `float4`
//     instead packs one element per register with all four components used, so
//     `float4 pack[6]` is 96 bytes — matching `float slot[24]` in
//     OspGfxUniforms byte for byte — and `res`, `time` and `pad` then land in
//     the seventh register's xy, z and w, again exactly the C layout.
//   * `mix` is `lerp`, `float2x2` is built from rows rather than columns and is
//     applied with `mul`, and `M_LOG2E_F` is a Metal standard-library spelling
//     with no HLSL equivalent.
//   * There is no `constant` address space. File-scope `static const` is the
//     equivalent; a plain global would silently become another uniform.
//   * A scalar splat is written `(float3)0.0`, and a scalar conversion is a
//     cast, `(float)i`, rather than the constructor call `float(i)` Metal
//     inherits from C++. Both spellings are accepted far less consistently
//     across shader compilers than the cast is.

#define OSP_GFX_SLOTS 24

// The packing argument above holds only if the slot count is a whole number of
// float4 registers. Otherwise it is not an error, it is a quiet half-register
// of padding that shifts `res`, `time` and every uniform after them.
#if (OSP_GFX_SLOTS % 4) != 0
#error OSP_GFX_SLOTS must be a multiple of 4 for the float4 packing below
#endif

#define OSP_GFX_SLOT_REGISTERS (OSP_GFX_SLOTS / 4)

// Bound as root constants, so the bridge writes OspGfxUniforms straight into
// the command list with no upload heap and no per-frame resource to fence on.
cbuffer OspGfxUniforms : register(b0) {
    float4 uniformSlotPack[OSP_GFX_SLOT_REGISTERS];
    float2 uniformRes;
    float uniformTime;
    float uniformPad;
};

// The bridge's OspGfxUniforms as the shader wants to see it. Unpacking once,
// here, is what lets every helper below take the same `Uniforms` its Metal twin
// takes instead of reaching into a differently shaped global; every slot index
// in this file is a compile-time constant, so the copy folds away entirely.
struct Uniforms {
    float slot[OSP_GFX_SLOTS];
    float2 res;
    float time;
    float pad;
};

Uniforms loadUniforms() {
    Uniforms u;
    [unroll]
    for (int i = 0; i < OSP_GFX_SLOTS; i++) {
        u.slot[i] = uniformSlotPack[i >> 2][i & 3];
    }
    u.res = uniformRes;
    u.time = uniformTime;
    u.pad = uniformPad;
    return u;
}

// Motion slots: what the host's fixed-point arithmetic moves each frame.
static const int SLOT_FOLD_X = 0;
static const int SLOT_FOLD_Y = 1;
static const int SLOT_ZOOM = 2;
static const int SLOT_SPIN = 3;
static const int SLOT_PALETTE = 4;
static const int SLOT_CORE = 5;
static const int SLOT_DRIFT_X = 6;
static const int SLOT_DRIFT_Y = 7;

// Look slots: the authored tuning, pushed once in thousandths.
static const int SLOT_FALLOFF = 8;
static const int SLOT_PALETTE_STEP = 9;
static const int SLOT_PALETTE_DEPTH = 10;
static const int SLOT_EXPOSURE = 11;
static const int SLOT_CORE_GAIN = 12;
static const int SLOT_CORE_FLOOR = 13;
static const int SLOT_CORE_SHARP = 14;
static const int SLOT_SATURATION = 15;
static const int SLOT_VIGNETTE = 16;
static const int SLOT_GAMMA = 17;
static const int SLOT_TRAP_CLAMP = 18;
static const int SLOT_BACKDROP_ZOOM = 19;
static const int SLOT_BACKDROP_DIM = 20;

static const float TAU = 6.2831853;
static const float FOLD_EPSILON = 1e-5;
static const float TRAP_SENTINEL = 1e9;
static const float3 LUMA = float3(0.299, 0.587, 0.114);
// Metal's standard library supplies this; HLSL does not. It must round to the
// same float or `falloff` stops agreeing with its Metal twin to the last bit.
static const float M_LOG2E_F = 1.44269504088896340736;

// ===========================================================================
// Shared helper layer. Nothing below this line knows which scene is drawing.
// ===========================================================================

// The same full-screen triangle from the vertex id alone. Direct3D and Metal
// agree on clip space here — y is up in both — so the identical arithmetic
// produces the identical coverage, and the bridge disables culling so the
// winding the two rasterisers call front-facing never enters into it.
float4 osp_vertex(uint vid : SV_VertexID) : SV_Position {
    float2 p = float2((float)((vid << 1) & 2), (float)(vid & 2));
    return float4(p * 2.0 - 1.0, 0.0, 1.0);
}

// Pixel centre to a y-normalised, origin-centred plane. `SV_Position` in a
// pixel shader carries the same half-pixel-offset, y-down convention Metal's
// [[position]] does, so this is a straight transcription.
float2 uvFromPosition(float4 pos, Uniforms u) {
    return (pos.xy * 2.0 - u.res) / u.res.y;
}

// Metal's float2x2 constructor takes columns and multiplies a column vector on
// its right; HLSL's takes rows and needs `mul`. Same rotation, transposed
// source text — the one helper where a careless transcription would silently
// spin every scene the wrong way and still compile.
float2x2 rotationFromTurns(float turns) {
    float a = turns * TAU;
    return float2x2(cos(a), -sin(a), sin(a), cos(a));
}

float3 palette(float t) {
    return 0.5 + 0.5 * cos(TAU * (t + float3(0.0, 0.33, 0.67)));
}

float cosineWave(float t) {
    return 0.5 + 0.5 * cos(TAU * t);
}

float2 kaliFold(float2 z, float2 c) {
    return abs(z) / max(dot(z, z), FOLD_EPSILON) - c;
}

// Written as the exp2 the GPU actually executes, folding log2(e) by hand, for
// the reason base.metal sets out: a rate arriving in a uniform is not folded
// for you, and the extra rounding drifts the image by an ulp.
float falloff(float rate, float d) {
    return exp2(-rate * M_LOG2E_F * d);
}

float3 saturation(float3 col, float amount) {
    return lerp((float3)dot(col, LUMA), col, amount);
}

float vignetteQuadratic(float2 uv, float amount) {
    return 1.0 - amount * dot(uv, uv);
}

float vignetteElliptical(float2 uv, float amount, float2 aspect,
                         float inner, float outer) {
    return 1.0 - amount * smoothstep(inner, outer, length(uv * aspect));
}

float3 reinhard(float3 col) {
    return col / (1.0 + col);
}

float3 aces(float3 x) {
    return clamp((x * (2.51 * x + 0.03)) /
                 (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

// `max` before `pow` because a negative channel raised to a fractional power is
// a NaN, and one NaN pixel reads as a hole in the image.
float3 gammaEncode(float3 col, float exponent) {
    return pow(max(col, 0.0), (float3)exponent);
}

float sdCircle(float2 p, float2 centre, float r) {
    return length(p - centre) - r;
}

float sdSegment(float2 p, float2 a, float2 b, float r) {
    float2 pa = p - a;
    float2 ba = b - a;
    float h = clamp(dot(pa, ba) / max(dot(ba, ba), FOLD_EPSILON), 0.0, 1.0);
    return length(pa - ba * h) - r;
}

float smoothUnion(float d1, float d2, float k) {
    float h = clamp(0.5 + 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return lerp(d2, d1, h) - k * h * (1.0 - h);
}

float2 foldPlane(float2 uv, Uniforms u) {
    return mul(rotationFromTurns(u.slot[SLOT_SPIN]), uv) * u.slot[SLOT_ZOOM] +
           float2(u.slot[SLOT_DRIFT_X], u.slot[SLOT_DRIFT_Y]);
}

float2 foldShape(Uniforms u) {
    return float2(u.slot[SLOT_FOLD_X], u.slot[SLOT_FOLD_Y]);
}

// ===========================================================================
// osp_fragment — Kali.
// ===========================================================================

// A compile-time constant so the loop can unroll: a property of the compiled
// kernel, not of a frame, which is why it is not a slot.
static const int KALI_ITERATIONS = 18;

struct KaliOrbit {
    float3 col;
    float trap;
};

KaliOrbit kaliOrbit(float2 z, float2 c, Uniforms u) {
    KaliOrbit orbit;
    orbit.col = (float3)0.0;
    orbit.trap = TRAP_SENTINEL;
    for (int i = 0; i < KALI_ITERATIONS; i++) {
        z = kaliFold(z, c);
        float d = min(length(z), u.slot[SLOT_TRAP_CLAMP]);
        orbit.trap = min(orbit.trap, d);
        orbit.col += palette(u.slot[SLOT_PALETTE_STEP] * (float)i +
                             u.slot[SLOT_PALETTE_DEPTH] * d +
                             u.slot[SLOT_PALETTE]) *
                     falloff(u.slot[SLOT_FALLOFF], d);
    }
    return orbit;
}

float3 kaliCore(float trap, Uniforms u) {
    return u.slot[SLOT_CORE] * palette(u.slot[SLOT_PALETTE] + 0.5) *
           u.slot[SLOT_CORE_GAIN] /
           (u.slot[SLOT_CORE_FLOOR] + u.slot[SLOT_CORE_SHARP] * trap * trap);
}

float4 osp_fragment(float4 pos : SV_Position) : SV_Target {
    Uniforms u = loadUniforms();
    float2 uv = uvFromPosition(pos, u);
    KaliOrbit orbit = kaliOrbit(foldPlane(uv, u), foldShape(u), u);
    float3 col = orbit.col * u.slot[SLOT_EXPOSURE] + kaliCore(orbit.trap, u);
    col = saturation(col, u.slot[SLOT_SATURATION]);
    col *= vignetteQuadratic(uv, u.slot[SLOT_VIGNETTE]);
    return float4(gammaEncode(reinhard(col), u.slot[SLOT_GAMMA]), 1.0);
}

// ===========================================================================
// osp_fragment_opal — Midnight Opal.
// ===========================================================================

static const int OPAL_ITERATIONS = 13;
static const float3 OPAL_INK = float3(0.005, 0.008, 0.025);
static const float3 OPAL_DEEP_PLUM = float3(0.018, 0.008, 0.042);
static const float3 OPAL_VIOLET = float3(0.40, 0.045, 0.64);
static const float3 OPAL_CYAN = float3(0.018, 0.56, 0.72);
static const float3 OPAL_PEARL = float3(0.68, 0.91, 1.00);
static const float3 OPAL_CORAL = float3(0.82, 0.12, 0.28);
static const float2 OPAL_COOL_CENTRE = float2(0.52, -0.30);
static const float2 OPAL_WARM_CENTRE = float2(-0.72, 0.45);
static const float2 OPAL_FOCUS_CENTRE = float2(-0.10, 0.04);
static const float2 OPAL_ASPECT = float2(0.62, 1.0);
static const float2 OPAL_COOL_ASPECT = float2(0.78, 1.0);
static const float2 OPAL_WARM_ASPECT = float2(0.72, 1.0);
static const float OPAL_DISPLAY_GAMMA = 1.0 / 2.2;
// Stays a literal for the reason base.metal gives: the compiler folds it into
// the ACES rational's coefficients, which a uniform cannot be.
static const float OPAL_EXPOSURE = 1.08;

struct OpalOrbit {
    float energy;
    float flow;
    float trap;
};

OpalOrbit opalOrbit(float2 z, float2 c, Uniforms u) {
    OpalOrbit orbit;
    orbit.energy = 0.0;
    orbit.flow = 0.0;
    orbit.trap = TRAP_SENTINEL;
    for (int i = 0; i < OPAL_ITERATIONS; i++) {
        z = kaliFold(z, c);
        float d = min(length(z), u.slot[SLOT_TRAP_CLAMP]);
        orbit.trap = min(orbit.trap, d);
        float e = falloff(u.slot[SLOT_FALLOFF], d) * (1.0 - 0.025 * (float)i);
        orbit.energy += e;
        orbit.flow += e * cosineWave(u.slot[SLOT_PALETTE_STEP] * (float)i +
                                     u.slot[SLOT_PALETTE_DEPTH] * d +
                                     u.slot[SLOT_PALETTE]);
    }
    return orbit;
}

float3 opalGround(float2 uv, float coolWash, float warmWash) {
    float vertical = saturate(0.52 - 0.20 * uv.y);
    float3 col = lerp(OPAL_INK, OPAL_DEEP_PLUM,
                      0.12 + 0.12 * vertical + 0.18 * warmWash);
    return col + float3(0.005, 0.014, 0.024) * coolWash;
}

float4 osp_fragment_opal(float4 pos : SV_Position) : SV_Target {
    Uniforms u = loadUniforms();
    float2 uv = uvFromPosition(pos, u);
    float2 coolPoint = (uv - OPAL_COOL_CENTRE) * OPAL_COOL_ASPECT;
    float2 warmPoint = (uv - OPAL_WARM_CENTRE) * OPAL_WARM_ASPECT;
    float2 focusPoint = (uv - OPAL_FOCUS_CENTRE) * OPAL_ASPECT;
    float3 col = opalGround(uv, exp(-2.70 * dot(coolPoint, coolPoint)),
                            exp(-3.20 * dot(warmPoint, warmPoint)));

    OpalOrbit orbit = opalOrbit(foldPlane(uv, u), foldShape(u), u);
    float3 silk = lerp(OPAL_VIOLET, OPAL_CYAN,
                       smoothstep(0.32, 0.68, orbit.flow / max(orbit.energy, 1e-4)));
    float focus = 0.54 + 0.58 * exp(-0.95 * dot(focusPoint, focusPoint));
    float core = exp(-800.0 * orbit.trap * orbit.trap);
    col += silk * (0.014 * (1.0 - exp(-0.08 * orbit.energy)) * focus);
    col += silk * (0.040 * (1.0 - exp(-0.40 * orbit.energy)) +
                   0.10 * exp(-14.0 * orbit.trap)) * focus;
    col += OPAL_PEARL * core * (0.018 + 0.035 * u.slot[SLOT_CORE]);
    col += OPAL_CORAL * (0.024 * exp(-1.35 * dot(warmPoint, warmPoint)) * core * core);
    col *= vignetteElliptical(uv, u.slot[SLOT_VIGNETTE], OPAL_ASPECT, 0.42, 1.48);
    return float4(gammaEncode(aces(col * OPAL_EXPOSURE), OPAL_DISPLAY_GAMMA), 1.0);
}

// ===========================================================================
// osp_fragment_character — a signed-distance character over a folded backdrop.
// ===========================================================================

static const int BACKDROP_ITERATIONS = 6;

// The proportions of the figure, authored y-up.
static const float2 CHARACTER_HEAD = float2(0.0, 0.30);
static const float CHARACTER_HEAD_R = 0.32;
static const float2 CHARACTER_SHOULDER = float2(0.0, -0.12);
static const float2 CHARACTER_HIP = float2(0.0, -0.50);
static const float CHARACTER_BODY_R = 0.24;
static const float2 CHARACTER_WING_ROOT = float2(0.19, -0.14);
static const float2 CHARACTER_WING_TIP = float2(0.42, -0.42);
static const float CHARACTER_WING_R = 0.075;
static const float2 CHARACTER_TUFT_ROOT = float2(0.13, 0.52);
static const float2 CHARACTER_TUFT_TIP = float2(0.27, 0.80);
static const float CHARACTER_TUFT_R = 0.045;
static const float CHARACTER_NECK_FILLET = 0.07;
static const float CHARACTER_WING_FILLET = 0.05;
static const float CHARACTER_TUFT_FILLET = 0.04;
static const float2 CHARACTER_EYE = float2(0.135, 0.36);
static const float CHARACTER_EYE_R = 0.075;
static const float2 CHARACTER_NOSE = float2(0.0, 0.20);
static const float CHARACTER_NOSE_R = 0.042;
static const float2 CHARACTER_GLINT = float2(0.162, 0.392);
static const float CHARACTER_GLINT_R = 0.024;
static const float CHARACTER_FACE_SOFT = 0.006;
static const float CHARACTER_SHADE_DEPTH = 0.30;
static const float CHARACTER_AMBIENT = 0.42;
static const float CHARACTER_LIGHT_LOW = -0.55;
static const float CHARACTER_LIGHT_HIGH = 0.45;
static const float3 CHARACTER_INK = float3(0.02, 0.03, 0.05);
static const float3 CHARACTER_GLINT_COLOUR = float3(1.0, 0.98, 0.94);

float characterField(float2 p) {
    float2 m = float2(abs(p.x), p.y);
    float head = sdCircle(p, CHARACTER_HEAD, CHARACTER_HEAD_R);
    float body = sdSegment(p, CHARACTER_SHOULDER, CHARACTER_HIP, CHARACTER_BODY_R);
    float wing = sdSegment(m, CHARACTER_WING_ROOT, CHARACTER_WING_TIP, CHARACTER_WING_R);
    float tuft = sdSegment(m, CHARACTER_TUFT_ROOT, CHARACTER_TUFT_TIP, CHARACTER_TUFT_R);
    float torso = smoothUnion(smoothUnion(head, body, CHARACTER_NECK_FILLET),
                              wing, CHARACTER_WING_FILLET);
    return smoothUnion(torso, tuft, CHARACTER_TUFT_FILLET);
}

float characterFace(float2 p) {
    float2 m = float2(abs(p.x), p.y);
    float eyes = sdCircle(m, CHARACTER_EYE, CHARACTER_EYE_R);
    float nose = sdCircle(p, CHARACTER_NOSE, CHARACTER_NOSE_R);
    return 1.0 - smoothstep(0.0, CHARACTER_FACE_SOFT, min(eyes, nose));
}

float characterGlint(float2 p) {
    float2 m = float2(abs(p.x), p.y);
    return 1.0 - smoothstep(0.0, CHARACTER_FACE_SOFT,
                            sdCircle(m, CHARACTER_GLINT, CHARACTER_GLINT_R));
}

float3 characterBackdrop(float2 uv, Uniforms u) {
    float2 z = mul(rotationFromTurns(u.slot[SLOT_PALETTE]), uv) *
               u.slot[SLOT_BACKDROP_ZOOM];
    float2 c = foldShape(u);
    float3 col = (float3)0.0;
    for (int i = 0; i < BACKDROP_ITERATIONS; i++) {
        z = kaliFold(z, c);
        float d = min(length(z), u.slot[SLOT_TRAP_CLAMP]);
        col += palette(u.slot[SLOT_PALETTE_STEP] * (float)i +
                       u.slot[SLOT_PALETTE_DEPTH] * d + u.slot[SLOT_PALETTE]) *
               falloff(u.slot[SLOT_FALLOFF], d);
    }
    return col * u.slot[SLOT_BACKDROP_DIM];
}

float3 characterShade(float3 back, float2 p, Uniforms u) {
    float d = characterField(p);
    float mask = 1.0 - smoothstep(0.0, u.slot[SLOT_CORE_FLOOR], d);
    float interior = clamp(-d / CHARACTER_SHADE_DEPTH, 0.0, 1.0);
    float key = CHARACTER_AMBIENT + (1.0 - CHARACTER_AMBIENT) *
                smoothstep(CHARACTER_LIGHT_LOW, CHARACTER_LIGHT_HIGH, p.y);
    float3 skin = lerp(palette(u.slot[SLOT_PALETTE] + u.slot[SLOT_PALETTE_STEP]),
                       palette(u.slot[SLOT_PALETTE]), interior) * key;
    float3 col = lerp(back, skin, mask);
    col = lerp(col, CHARACTER_INK, characterFace(p) * mask);
    col += CHARACTER_GLINT_COLOUR * characterGlint(p) * mask;
    float rim = falloff(u.slot[SLOT_CORE_SHARP], abs(d)) * u.slot[SLOT_CORE_GAIN];
    return col + skin * rim * u.slot[SLOT_CORE];
}

float4 osp_fragment_character(float4 pos : SV_Position) : SV_Target {
    Uniforms u = loadUniforms();
    float2 uv = uvFromPosition(pos, u);
    float2 q = mul(rotationFromTurns(u.slot[SLOT_SPIN]), uv) * u.slot[SLOT_ZOOM] -
               float2(u.slot[SLOT_DRIFT_X], u.slot[SLOT_DRIFT_Y]);
    // SV_Position grows downward exactly as Metal's [[position]] does; the
    // figure is authored y-up, and this is where the two conventions meet.
    float3 col = characterShade(characterBackdrop(uv, u), float2(q.x, -q.y), u);
    col = saturation(col, u.slot[SLOT_SATURATION]);
    col *= vignetteQuadratic(uv, u.slot[SLOT_VIGNETTE]);
    return float4(gammaEncode(aces(col * u.slot[SLOT_EXPOSURE]),
                              u.slot[SLOT_GAMMA]), 1.0);
}
