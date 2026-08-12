// The Osprey Metal library: one translation unit, one `Uniforms` layout, one
// full-screen vertex stage, one reusable helper layer, and every named fragment
// entry point the graphics examples select between. `osp_gfx_open` compiles this
// file with `newLibraryWithSource:` at run time and picks a fragment by name, so
// a scene is a name plus a table of numbers — not another copy of a shader.
//
// A run-time `newLibraryWithSource:` has no include path, so `#include "..."`
// cannot resolve here. Sharing therefore means *one file*, not a header: this
// is the only place the slot layout and the fold are written down.
//
// WHERE THE DIVIDING LINE FALLS. Osprey owns every number that moves and every
// number a scene author turns to change how a scene reads; those arrive in
// `slot`, pushed by the host. What stays in this file is:
//   * mathematics — `TAU`, the fold epsilon, the orbit-trap sentinel;
//   * loop trip counts — they must be compile-time constants or the loop cannot
//     unroll, so they are a property of the compiled kernel, not of a frame;
//   * the fixed palette identity of a composition — the named colours that *are*
//     a piece and never vary.
// Anything else in a slot.

#include <metal_stdlib>
using namespace metal;

// `Uniforms` mirrors `OspGfxUniforms` in ospgfx.m field for field. Both sides
// spell the slot count out, and `crates/osprey-cli/tests/graphics_scenes.rs`
// asserts the two agree: a silent disagreement here does not fail to compile,
// it quietly shifts `res` and `time` and corrupts every scene at once.
#define OSP_GFX_SLOTS 24

struct Uniforms {
    float slot[OSP_GFX_SLOTS];
    float2 res;
    float time;
    float pad;
};

// Motion slots: what the host's fixed-point arithmetic moves each frame. Every
// scene agrees on these seven meanings, which is why one Osprey loop drives all
// of them. Angles arrive already normalised to turns, so 1.0 is a full circle.
constant int SLOT_FOLD_X = 0;
constant int SLOT_FOLD_Y = 1;
constant int SLOT_ZOOM = 2;
constant int SLOT_SPIN = 3;
constant int SLOT_PALETTE = 4;
constant int SLOT_CORE = 5;
constant int SLOT_DRIFT_X = 6;
constant int SLOT_DRIFT_Y = 7;

// Look slots: the authored tuning that used to be baked into the shader source.
// The host pushes these once, in thousandths, so a written 0.085 survives the
// trip to the GPU as exactly the float the literal 0.085 would have produced.
constant int SLOT_FALLOFF = 8;
constant int SLOT_PALETTE_STEP = 9;
constant int SLOT_PALETTE_DEPTH = 10;
constant int SLOT_EXPOSURE = 11;
constant int SLOT_CORE_GAIN = 12;
constant int SLOT_CORE_FLOOR = 13;
constant int SLOT_CORE_SHARP = 14;
constant int SLOT_SATURATION = 15;
constant int SLOT_VIGNETTE = 16;
constant int SLOT_GAMMA = 17;
constant int SLOT_TRAP_CLAMP = 18;
constant int SLOT_BACKDROP_ZOOM = 19;
constant int SLOT_BACKDROP_DIM = 20;

constant float TAU = 6.2831853;
// The fold divides by dot(z, z); this floor is what keeps the pixel at the
// origin finite instead of returning a NaN that poisons the whole orbit.
constant float FOLD_EPSILON = 1e-5;
// Larger than any clamped distance, so the first iteration always wins the min.
constant float TRAP_SENTINEL = 1e9;
// Rec. 601 luma, the weighting the saturation mix desaturates towards.
constant float3 LUMA = float3(0.299, 0.587, 0.114);

// ===========================================================================
// Shared helper layer. Nothing below this line knows which scene is drawing.
// ===========================================================================

// A full-screen triangle, built from the vertex id alone — no vertex buffer.
// Every fragment entry in this library shares it.
vertex float4 osp_vertex(uint vid [[vertex_id]]) {
    float2 p = float2(float((vid << 1) & 2), float(vid & 2));
    return float4(p * 2.0 - 1.0, 0.0, 1.0);
}

// Pixel centre to a y-normalised, origin-centred plane: y runs -1..1 whatever
// the window's aspect, so a scene composes in units it can reason about.
static float2 uvFromPosition(float4 pos, constant Uniforms &u) {
    return (pos.xy * 2.0 - u.res) / u.res.y;
}

// A rotation from a slot carrying turns rather than radians. The host works in
// turn units because they wrap with a cheap integer modulo.
static float2x2 rotationFromTurns(float turns) {
    float a = turns * TAU;
    return float2x2(cos(a), sin(a), -sin(a), cos(a));
}

// Iñigo Quílez's cosine palette: three phase-shifted cosines sweeping a smooth
// closed loop through colour space.
static float3 palette(float t) {
    return 0.5 + 0.5 * cos(TAU * (t + float3(0.0, 0.33, 0.67)));
}

// The same cosine as a scalar, for scenes that steer a mix rather than a hue.
static float cosineWave(float t) {
    return 0.5 + 0.5 * cos(TAU * t);
}

// One step of the Kali fold, `z = |z| / dot(z, z) - c`. The reciprocal turns the
// plane inside out around the origin, and the absolute value folds it into a
// quadrant, so a handful of steps weaves filament structure out of nothing.
// `c` is the shape, and the host moves it.
static float2 kaliFold(float2 z, float2 c) {
    return abs(z) / max(dot(z, z), FOLD_EPSILON) - c;
}

// Exponential falloff over a distance, written as the exp2 the GPU actually
// executes. Given a literal rate the compiler folds `rate * log2(e)` into one
// constant; folding it by hand keeps a rate pushed through a slot bit-identical
// to the same rate written inline, instead of letting the extra rounding drift
// the image by an ulp. That is what makes moving these numbers into Osprey a
// pure refactor rather than a re-grade.
static float falloff(float rate, float d) {
    return exp2(-rate * M_LOG2E_F * d);
}

// Push colour away from (or towards) its own luma. Above 1.0 this saturates.
static float3 saturation(float3 col, float amount) {
    return mix(float3(dot(col, LUMA)), col, amount);
}

// The plain quadratic vignette: cheap, and it reaches black in the corners.
static float vignetteQuadratic(float2 uv, float amount) {
    return 1.0 - amount * dot(uv, uv);
}

// An elliptical vignette for wide canvases, which never clips through black at
// the corners the way the quadratic one does.
static float vignetteElliptical(float2 uv, float amount, float2 aspect,
                                float inner, float outer) {
    return 1.0 - amount * smoothstep(inner, outer, length(uv * aspect));
}

// Reinhard: the simplest tonemap that is monotone and never clips.
static float3 reinhard(float3 col) {
    return col / (1.0 + col);
}

// A compact ACES approximation. It keeps bright filaments pearlescent instead
// of clipping each channel independently into harsh neon.
static float3 aces(float3 x) {
    return clamp((x * (2.51 * x + 0.03)) /
                 (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

// Display transfer. `max` before `pow` because a negative channel raised to a
// fractional power is a NaN, and one NaN pixel reads as a hole in the image.
static float3 gammaEncode(float3 col, float exponent) {
    return pow(max(col, 0.0), float3(exponent));
}

// Signed distance to a circle, and to a capsule between two points. Negative
// inside, zero on the surface, and the value is a true distance, so `exp` and
// `smoothstep` over it behave predictably.
static float sdCircle(float2 p, float2 centre, float r) {
    return length(p - centre) - r;
}

static float sdSegment(float2 p, float2 a, float2 b, float r) {
    float2 pa = p - a;
    float2 ba = b - a;
    float h = clamp(dot(pa, ba) / max(dot(ba, ba), FOLD_EPSILON), 0.0, 1.0);
    return length(pa - ba * h) - r;
}

// Polynomial smooth minimum: a union whose seam is a fillet of radius `k`, so
// two primitives read as one body rather than as two shapes touching.
static float smoothUnion(float d1, float d2, float k) {
    float h = clamp(0.5 + 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) - k * h * (1.0 - h);
}

// The plane a scene folds, placed by the three framing slots every scene shares.
static float2 foldPlane(float2 uv, constant Uniforms &u) {
    return rotationFromTurns(u.slot[SLOT_SPIN]) * uv * u.slot[SLOT_ZOOM] +
           float2(u.slot[SLOT_DRIFT_X], u.slot[SLOT_DRIFT_Y]);
}

static float2 foldShape(constant Uniforms &u) {
    return float2(u.slot[SLOT_FOLD_X], u.slot[SLOT_FOLD_Y]);
}

// ===========================================================================
// osp_fragment — Kali. The original scene, now reading its tuning from slots.
// ===========================================================================

// The trip count is deliberately not a slot: it decides whether this loop
// unrolls, which makes it a property of the compiled kernel and not of a frame.
constant int KALI_ITERATIONS = 18;

struct KaliOrbit {
    float3 col;
    float trap;
};

// Accumulate light along the orbit, and remember how close it ever came to the
// fold's fixed point. The sharp exponential falloff is what keeps the field
// black: only iterations that land near that point contribute anything at all.
static KaliOrbit kaliOrbit(float2 z, float2 c, constant Uniforms &u) {
    KaliOrbit orbit;
    orbit.col = float3(0.0);
    orbit.trap = TRAP_SENTINEL;
    for (int i = 0; i < KALI_ITERATIONS; i++) {
        z = kaliFold(z, c);
        float d = min(length(z), u.slot[SLOT_TRAP_CLAMP]);
        orbit.trap = min(orbit.trap, d);
        orbit.col += palette(u.slot[SLOT_PALETTE_STEP] * float(i) +
                             u.slot[SLOT_PALETTE_DEPTH] * d +
                             u.slot[SLOT_PALETTE]) *
                     falloff(u.slot[SLOT_FALLOFF], d);
    }
    return orbit;
}

// The reciprocal of the orbit trap is the neon core running down each filament:
// rare, tiny, and the only part of the image allowed to reach full brightness.
static float3 kaliCore(float trap, constant Uniforms &u) {
    return u.slot[SLOT_CORE] * palette(u.slot[SLOT_PALETTE] + 0.5) *
           u.slot[SLOT_CORE_GAIN] /
           (u.slot[SLOT_CORE_FLOOR] + u.slot[SLOT_CORE_SHARP] * trap * trap);
}

fragment float4 osp_fragment(float4 pos [[position]],
                             constant Uniforms &u [[buffer(0)]]) {
    float2 uv = uvFromPosition(pos, u);
    KaliOrbit orbit = kaliOrbit(foldPlane(uv, u), foldShape(u), u);
    float3 col = orbit.col * u.slot[SLOT_EXPOSURE] + kaliCore(orbit.trap, u);
    col = saturation(col, u.slot[SLOT_SATURATION]);
    col *= vignetteQuadratic(uv, u.slot[SLOT_VIGNETTE]);
    return float4(gammaEncode(reinhard(col), u.slot[SLOT_GAMMA]), 1.0);
}

// ===========================================================================
// osp_fragment_opal — Midnight Opal. A fixed duotone composition: its colours
// and its placements are its identity and never move, so they stay here; the
// four numbers that set its dynamic range come from slots like every scene's.
// ===========================================================================

constant int OPAL_ITERATIONS = 13;
constant float3 OPAL_INK = float3(0.005, 0.008, 0.025);
constant float3 OPAL_DEEP_PLUM = float3(0.018, 0.008, 0.042);
constant float3 OPAL_VIOLET = float3(0.40, 0.045, 0.64);
constant float3 OPAL_CYAN = float3(0.018, 0.56, 0.72);
constant float3 OPAL_PEARL = float3(0.68, 0.91, 1.00);
constant float3 OPAL_CORAL = float3(0.82, 0.12, 0.28);
constant float2 OPAL_COOL_CENTRE = float2(0.52, -0.30);
constant float2 OPAL_WARM_CENTRE = float2(-0.72, 0.45);
constant float2 OPAL_FOCUS_CENTRE = float2(-0.10, 0.04);
constant float2 OPAL_ASPECT = float2(0.62, 1.0);
constant float2 OPAL_COOL_ASPECT = float2(0.78, 1.0);
constant float2 OPAL_WARM_ASPECT = float2(0.72, 1.0);
// sRGB-ish display transfer: a property of the display, not of the piece.
constant float OPAL_DISPLAY_GAMMA = 1.0 / 2.2;
// The one range control this scene keeps. The compiler folds a literal exposure
// straight into the ACES rational's coefficients, which a uniform cannot be, so
// leaving it here is what makes this entry bit-identical to the shader it
// replaced. Its four other range controls fold cleanly and come from slots.
constant float OPAL_EXPOSURE = 1.08;

struct OpalOrbit {
    float energy;
    float flow;
    float trap;
};

// The orbit contributes bounded scalar fields instead of raw RGB, which keeps
// the broad woven haze, the readable filaments, and the rare bright core as
// three things the composition below can weight independently.
static OpalOrbit opalOrbit(float2 z, float2 c, constant Uniforms &u) {
    OpalOrbit orbit;
    orbit.energy = 0.0;
    orbit.flow = 0.0;
    orbit.trap = TRAP_SENTINEL;
    for (int i = 0; i < OPAL_ITERATIONS; i++) {
        z = kaliFold(z, c);
        float d = min(length(z), u.slot[SLOT_TRAP_CLAMP]);
        orbit.trap = min(orbit.trap, d);
        float e = falloff(u.slot[SLOT_FALLOFF], d) * (1.0 - 0.025 * float(i));
        orbit.energy += e;
        orbit.flow += e * cosineWave(u.slot[SLOT_PALETTE_STEP] * float(i) +
                                     u.slot[SLOT_PALETTE_DEPTH] * d +
                                     u.slot[SLOT_PALETTE]);
    }
    return orbit;
}

// Midnight ink, lifted by two broad pools of light. Giving the voids a
// deliberate atmosphere is what lets them work as negative space.
static float3 opalGround(float2 uv, float coolWash, float warmWash) {
    float vertical = saturate(0.52 - 0.20 * uv.y);
    float3 col = mix(OPAL_INK, OPAL_DEEP_PLUM,
                     0.12 + 0.12 * vertical + 0.18 * warmWash);
    return col + float3(0.005, 0.014, 0.024) * coolWash;
}

fragment float4 osp_fragment_opal(float4 pos [[position]],
                                  constant Uniforms &u [[buffer(0)]]) {
    float2 uv = uvFromPosition(pos, u);
    float2 coolPoint = (uv - OPAL_COOL_CENTRE) * OPAL_COOL_ASPECT;
    float2 warmPoint = (uv - OPAL_WARM_CENTRE) * OPAL_WARM_ASPECT;
    float2 focusPoint = (uv - OPAL_FOCUS_CENTRE) * OPAL_ASPECT;
    float3 col = opalGround(uv, exp(-2.70 * dot(coolPoint, coolPoint)),
                            exp(-3.20 * dot(warmPoint, warmPoint)));

    OpalOrbit orbit = opalOrbit(foldPlane(uv, u), foldShape(u), u);
    float3 silk = mix(OPAL_VIOLET, OPAL_CYAN,
                      smoothstep(0.32, 0.68, orbit.flow / max(orbit.energy, 1e-4)));
    float focus = 0.54 + 0.58 * exp(-0.95 * dot(focusPoint, focusPoint));
    float core = exp(-800.0 * orbit.trap * orbit.trap);
    col += silk * (0.014 * (1.0 - exp(-0.08 * orbit.energy)) * focus);
    col += silk * (0.040 * (1.0 - exp(-0.40 * orbit.energy)) +
                   0.10 * exp(-14.0 * orbit.trap)) * focus;
    col += OPAL_PEARL * core * (0.018 + 0.035 * u.slot[SLOT_CORE]);

    // One very quiet warm glint keeps the duotone from feeling synthetic while
    // preserving the cyan-violet identity of the piece.
    col += OPAL_CORAL * (0.024 * exp(-1.35 * dot(warmPoint, warmPoint)) * core * core);
    col *= vignetteElliptical(uv, u.slot[SLOT_VIGNETTE], OPAL_ASPECT, 0.42, 1.48);
    return float4(gammaEncode(aces(col * OPAL_EXPOSURE), OPAL_DISPLAY_GAMMA), 1.0);
}

// ===========================================================================
// osp_fragment_character — a signed-distance character standing in front of a
// folded backdrop. It exists to prove the helper layer above is genuinely
// reusable: every primitive, every grade and the fold itself are shared code,
// and the only thing this scene adds is a silhouette.
// ===========================================================================

constant int BACKDROP_ITERATIONS = 6;

// The proportions of the figure, authored y-up. These are the piece's identity:
// moving them makes a different creature, not a differently-lit one.
constant float2 CHARACTER_HEAD = float2(0.0, 0.30);
constant float CHARACTER_HEAD_R = 0.32;
constant float2 CHARACTER_SHOULDER = float2(0.0, -0.12);
constant float2 CHARACTER_HIP = float2(0.0, -0.50);
constant float CHARACTER_BODY_R = 0.24;
constant float2 CHARACTER_WING_ROOT = float2(0.19, -0.14);
constant float2 CHARACTER_WING_TIP = float2(0.42, -0.42);
constant float CHARACTER_WING_R = 0.075;
constant float2 CHARACTER_TUFT_ROOT = float2(0.13, 0.52);
constant float2 CHARACTER_TUFT_TIP = float2(0.27, 0.80);
constant float CHARACTER_TUFT_R = 0.045;
constant float CHARACTER_NECK_FILLET = 0.07;
constant float CHARACTER_WING_FILLET = 0.05;
constant float CHARACTER_TUFT_FILLET = 0.04;
constant float2 CHARACTER_EYE = float2(0.135, 0.36);
constant float CHARACTER_EYE_R = 0.075;
constant float2 CHARACTER_NOSE = float2(0.0, 0.20);
constant float CHARACTER_NOSE_R = 0.042;
constant float2 CHARACTER_GLINT = float2(0.162, 0.392);
constant float CHARACTER_GLINT_R = 0.024;
constant float CHARACTER_FACE_SOFT = 0.006;
// How far under the silhouette the body reaches its deepest hue, and how much
// light the underside keeps once the top-down key light has fallen off.
constant float CHARACTER_SHADE_DEPTH = 0.30;
constant float CHARACTER_AMBIENT = 0.42;
constant float CHARACTER_LIGHT_LOW = -0.55;
constant float CHARACTER_LIGHT_HIGH = 0.45;
constant float3 CHARACTER_INK = float3(0.02, 0.03, 0.05);
constant float3 CHARACTER_GLINT_COLOUR = float3(1.0, 0.98, 0.94);

// Head, body, wings and crest smooth-unioned into a single silhouette, so the
// joins read as anatomy rather than as primitives that happen to overlap.
// Mirroring x costs one `abs` and gives the paired limbs from one primitive
// each — the standard signed-distance trick for a bilateral creature.
static float characterField(float2 p) {
    float2 m = float2(abs(p.x), p.y);
    float head = sdCircle(p, CHARACTER_HEAD, CHARACTER_HEAD_R);
    float body = sdSegment(p, CHARACTER_SHOULDER, CHARACTER_HIP, CHARACTER_BODY_R);
    float wing = sdSegment(m, CHARACTER_WING_ROOT, CHARACTER_WING_TIP, CHARACTER_WING_R);
    float tuft = sdSegment(m, CHARACTER_TUFT_ROOT, CHARACTER_TUFT_TIP, CHARACTER_TUFT_R);
    float torso = smoothUnion(smoothUnion(head, body, CHARACTER_NECK_FILLET),
                              wing, CHARACTER_WING_FILLET);
    return smoothUnion(torso, tuft, CHARACTER_TUFT_FILLET);
}

// The face is inked onto the silhouette rather than cut out of it, so the eyes
// stay put when the rim light swells.
static float characterFace(float2 p) {
    float2 m = float2(abs(p.x), p.y);
    float eyes = sdCircle(m, CHARACTER_EYE, CHARACTER_EYE_R);
    float nose = sdCircle(p, CHARACTER_NOSE, CHARACTER_NOSE_R);
    return 1.0 - smoothstep(0.0, CHARACTER_FACE_SOFT, min(eyes, nose));
}

// One off-centre catchlight per eye. It is the whole difference between a
// creature that is looking at you and two black dots.
static float characterGlint(float2 p) {
    float2 m = float2(abs(p.x), p.y);
    return 1.0 - smoothstep(0.0, CHARACTER_FACE_SOFT,
                            sdCircle(m, CHARACTER_GLINT, CHARACTER_GLINT_R));
}

// A few folds, dimmed hard, so the figure has weather behind it rather than a
// flat fill. The host owns both the framing and how far down it is mixed.
static float3 characterBackdrop(float2 uv, constant Uniforms &u) {
    float2 z = rotationFromTurns(u.slot[SLOT_PALETTE]) * uv *
               u.slot[SLOT_BACKDROP_ZOOM];
    float2 c = foldShape(u);
    float3 col = float3(0.0);
    for (int i = 0; i < BACKDROP_ITERATIONS; i++) {
        z = kaliFold(z, c);
        float d = min(length(z), u.slot[SLOT_TRAP_CLAMP]);
        col += palette(u.slot[SLOT_PALETTE_STEP] * float(i) +
                       u.slot[SLOT_PALETTE_DEPTH] * d + u.slot[SLOT_PALETTE]) *
               falloff(u.slot[SLOT_FALLOFF], d);
    }
    return col * u.slot[SLOT_BACKDROP_DIM];
}

// Lay the silhouette over the backdrop. The body runs from a lighter edge hue
// to the palette's own colour deep inside, under a top-down key light, and the
// host pulses a rim that also spills into the backdrop as a halo.
static float3 characterShade(float3 back, float2 p, constant Uniforms &u) {
    float d = characterField(p);
    float mask = 1.0 - smoothstep(0.0, u.slot[SLOT_CORE_FLOOR], d);
    float interior = clamp(-d / CHARACTER_SHADE_DEPTH, 0.0, 1.0);
    float key = CHARACTER_AMBIENT + (1.0 - CHARACTER_AMBIENT) *
                smoothstep(CHARACTER_LIGHT_LOW, CHARACTER_LIGHT_HIGH, p.y);
    float3 skin = mix(palette(u.slot[SLOT_PALETTE] + u.slot[SLOT_PALETTE_STEP]),
                      palette(u.slot[SLOT_PALETTE]), interior) * key;
    float3 col = mix(back, skin, mask);
    col = mix(col, CHARACTER_INK, characterFace(p) * mask);
    col += CHARACTER_GLINT_COLOUR * characterGlint(p) * mask;
    float rim = falloff(u.slot[SLOT_CORE_SHARP], abs(d)) * u.slot[SLOT_CORE_GAIN];
    return col + skin * rim * u.slot[SLOT_CORE];
}

fragment float4 osp_fragment_character(float4 pos [[position]],
                                       constant Uniforms &u [[buffer(0)]]) {
    float2 uv = uvFromPosition(pos, u);
    float2 q = rotationFromTurns(u.slot[SLOT_SPIN]) * uv * u.slot[SLOT_ZOOM] -
               float2(u.slot[SLOT_DRIFT_X], u.slot[SLOT_DRIFT_Y]);
    // Metal's framebuffer y grows downward; the figure is authored y-up, and
    // this is the one place the two conventions meet.
    float3 col = characterShade(characterBackdrop(uv, u), float2(q.x, -q.y), u);
    col = saturation(col, u.slot[SLOT_SATURATION]);
    col *= vignetteQuadratic(uv, u.slot[SLOT_VIGNETTE]);
    return float4(gammaEncode(aces(col * u.slot[SLOT_EXPOSURE]),
                              u.slot[SLOT_GAMMA]), 1.0);
}
