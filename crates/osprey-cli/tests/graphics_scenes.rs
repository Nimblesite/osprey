//! Regression coverage for the shared real-time graphics examples.
//!
//! `examples/graphics` is one shared Osprey host (`base/base.osp`), one shader
//! library per GPU API (`base.metal`, `base.hlsl`), one bridge per platform,
//! and one thin entry per scene. Four things can silently rot in that
//! arrangement, and this file pins all four: a scene can start duplicating the
//! host or the shader, the original Kali scene can drift off its committed
//! timings and grade, `Uniforms` in a shader can stop matching `OspGfxUniforms`
//! in its bridge, and — the failure this file exists for now that there are two
//! backends — Windows and macOS can quietly stop rendering the same thing.
//!
//! Only the macOS backend is built and observed on the machine that wrote the
//! Windows one, so these are text assertions, not pixels. They catch drift.
//! They do not catch a shader that will not compile; `examples/graphics/README.md`
//! lists the Windows commands that do.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Slots 0..=7 carry motion; 8..=20 carry the authored grade. `base/base.osp`
/// and `base.metal` both spell this out, and the bridge sizes its array to it.
const LOOK_SLOT_LAST: usize = 20;

/// A scene entry is an import, a comment, and one call. Anything longer means
/// the shared host has started leaking back into the scenes.
const MAX_SCENE_SOURCE_LINES: usize = 3;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Read a file that the layout requires to exist. A missing or unreadable file
/// is a failed assertion with the path in it, never a panic.
fn read_required(path: &Path) -> String {
    let source = fs::read_to_string(path);
    assert!(
        source.is_ok(),
        "required graphics source is missing or unreadable: {}",
        path.display()
    );
    source.unwrap_or_default()
}

fn source_lines(source: &str) -> usize {
    source
        .lines()
        .filter(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with("//")
        })
        .count()
}

/// The integer following `needle` in `source`, e.g. the `24` in
/// `#define OSP_GFX_SLOTS 24`.
fn number_after(source: &str, needle: &str) -> Option<usize> {
    source.split(needle).nth(1).and_then(|rest| {
        let digits: String = rest
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok()
    })
}

#[test]
fn graphics_scenes_share_one_host_and_one_shader() {
    let graphics = repo_root().join("examples/graphics");
    let host = read_required(&graphics.join("base/base.osp"));

    for (scene, entry) in [
        ("kali", "SceneBase::original()"),
        ("opal", "SceneBase::opal()"),
        ("character", "SceneBase::character()"),
    ] {
        let source = read_required(&graphics.join(scene).join("scene.osp"));
        assert!(
            source.contains("import graphics::SceneBase"),
            "{scene} must import the shared host"
        );
        assert!(source.contains(entry), "{scene} must call {entry}");
        let lines = source_lines(&source);
        assert!(
            lines <= MAX_SCENE_SOURCE_LINES,
            "{scene} must stay a thin entry point, got {lines} source lines"
        );

        // Default Osprey cannot import across standalone scripts: an import
        // needs a project [MODULES-MODEL], and a project has one entry
        // [MODULES-ENTRYPOINT]. Each scene is therefore its own one-file
        // project naming the shared host's directory as a second source root.
        let manifest = read_required(&graphics.join(scene).join("osprey.toml"));
        assert!(
            manifest.contains("source_roots = [\"../base\", \".\"]"),
            "{scene} must take the shared host in as a second source root"
        );
        assert!(
            manifest.contains("entry = \"scene.osp\""),
            "{scene} must name its single entry source"
        );

        // The fixed-point trigonometry belongs to the host and nowhere else.
        for shared in [
            "fn swing(",
            "fn fsin(",
            "fn wrapTurn(",
            "extern fn osp_gfx_",
        ] {
            assert!(
                !source.contains(shared),
                "{scene} duplicates `{shared}` instead of using the shared host"
            );
        }
    }

    for gone in ["scene.osp", "scene2.osp", "scene.metal", "scene2.metal"] {
        assert!(
            !graphics.join(gone).exists(),
            "{gone} must not come back: scenes share base/base.osp and base.metal"
        );
    }

    // These are the committed Kali timings. The shared host carries other
    // profiles, but the original must continue to use these exact values.
    for exact_original in [
        "0 => swing(t, 27000, 4055, 1105)",
        "1 => sway(t, 37000, 3768, 1064)",
        "2 => swing(t, 41000, 4300, 1350)",
        "3 => phase(t, 73000)",
        "4 => phase(t, 19000)",
        "5 => swing(t, 11000, 1150, 720)",
        "6 => swing(t, 53000, 0, 700)",
        "_ => sway(t, 61000, 0, 620)",
    ] {
        assert!(
            host.contains(exact_original),
            "original host behaviour changed or is missing: {exact_original}"
        );
    }
}

#[test]
fn kali_grade_moved_into_osprey_without_changing_a_pixel() {
    let graphics = repo_root().join("examples/graphics");
    let host = read_required(&graphics.join("base/base.osp"));
    let shader = read_required(&graphics.join("base.metal"));

    // The grade the shader used to hardcode, now pushed from Osprey in
    // thousandths so each value divides back to exactly the float the literal
    // produced: falloff, palette step and depth, exposure, core gain, floor and
    // sharpness, saturation, vignette, gamma, and the orbit-trap clamp.
    for (slot, thousandths) in [
        (8, 3600),
        (9, 85),
        (10, 450),
        (11, 420),
        (12, 90),
        (13, 6),
        (14, 3000),
        (15, 1450),
        (16, 300),
        (17, 720),
        (18, 24000),
    ] {
        assert!(
            host.contains(&format!("{slot} => {thousandths}")),
            "Kali's slot {slot} must still be pushed as {thousandths} thousandths"
        );
    }

    // None of those may be written back into the shader as a literal.
    for reverted in [
        "exp(-3.6",
        "0.085 * float(i)",
        "0.45 * d",
        "col * 0.42",
        "* 0.09 /",
        "0.006 + 3.0 * trap",
        "col, 1.45)",
        "0.30 * dot(uv, uv)",
        "float3(0.72)",
        "length(z), 24.0",
    ] {
        assert!(
            !shader.contains(reverted),
            "base.metal took an authored constant back off Osprey: {reverted}"
        );
    }

    // What legitimately stays in the shader: mathematics, and the trip count,
    // which has to be a compile-time constant for the loop to unroll.
    for kept in [
        "constant float TAU = 6.2831853;",
        "constant float FOLD_EPSILON = 1e-5;",
        "constant int KALI_ITERATIONS = 18;",
    ] {
        assert!(
            shader.contains(kept),
            "base.metal must keep the genuinely fixed constant: {kept}"
        );
    }

    // The shape of the original grade, expressed against slots. Reinhard, the
    // quadratic vignette and the luma-mix saturation are what make it Kali.
    for structural in [
        "fragment float4 osp_fragment(",
        "for (int i = 0; i < KALI_ITERATIONS; i++)",
        "falloff(u.slot[SLOT_FALLOFF], d)",
        "u.slot[SLOT_CORE_FLOOR] + u.slot[SLOT_CORE_SHARP] * trap * trap",
        "saturation(col, u.slot[SLOT_SATURATION])",
        "vignetteQuadratic(uv, u.slot[SLOT_VIGNETTE])",
        "gammaEncode(reinhard(col), u.slot[SLOT_GAMMA])",
    ] {
        assert!(
            shader.contains(structural),
            "original Metal behaviour changed or is missing: {structural}"
        );
    }

    // `exp(rate * d)` with a literal rate is folded by the compiler into an
    // `exp2` of one pre-multiplied constant; a rate arriving in a uniform is
    // not, and the extra rounding shifts the image by an ulp. Folding it by
    // hand is the reason moving the falloff into Osprey changed no pixel, so
    // the helper existing is part of the contract, not an implementation whim.
    assert!(
        shader.contains("exp2(-rate * M_LOG2E_F * d)"),
        "the falloff helper must keep folding log2(e) by hand"
    );
}

#[test]
fn every_named_fragment_entry_lives_in_both_shader_libraries() {
    let graphics = repo_root().join("examples/graphics");
    let metal = read_required(&graphics.join("base.metal"));
    let hlsl = read_required(&graphics.join("base.hlsl"));
    let host = read_required(&graphics.join("base/base.osp"));

    // One vertex stage and one uniform layout per library: each bridge
    // hardcodes the vertex entry's name and binds a single constant block.
    for (source, dialect, vertex) in [
        (&metal, "base.metal", "vertex float4 osp_vertex("),
        (
            &hlsl,
            "base.hlsl",
            "float4 osp_vertex(uint vid : SV_VertexID)",
        ),
    ] {
        assert_eq!(
            source.matches(vertex).count(),
            1,
            "{dialect} must declare exactly one osp_vertex"
        );
        assert_eq!(
            source.matches("struct Uniforms {").count(),
            1,
            "{dialect} must declare exactly one Uniforms"
        );
    }

    // A scene names its entry once and gets it on either platform, so an entry
    // present in one library and absent from the other is a scene that only
    // opens on one operating system.
    for entry in FRAGMENT_ENTRIES {
        assert!(
            metal.contains(&format!("fragment float4 {entry}(")),
            "base.metal must expose named fragment entry {entry}"
        );
        assert!(
            hlsl.contains(&format!("float4 {entry}(float4 pos : SV_Position)")),
            "base.hlsl must expose the same named fragment entry {entry}"
        );
        assert!(
            host.contains(&format!("\"{entry}\"")),
            "the shared host must open a scene on {entry}"
        );
    }
}

/// Shortest identifier `shader_constants` accepts, which keeps stray swizzles
/// and one-letter locals out of the comparison.
const SHORTEST_CONSTANT: usize = 3;

/// The three fragment entries every backend exposes; the bridges select between
/// them by name at run time.
const FRAGMENT_ENTRIES: [&str; 3] = [
    "osp_fragment",
    "osp_fragment_opal",
    "osp_fragment_character",
];

/// The whole Osprey-facing contract, in the order `base/base.osp` declares it.
const BRIDGE_EXPORTS: [&str; 6] = [
    "osp_gfx_open",
    "osp_gfx_set",
    "osp_gfx_set_milli",
    "osp_gfx_draw",
    "osp_gfx_ticks",
    "osp_gfx_close",
];

/// Drop `//` comments, so prose about a constant is never mistaken for the
/// constant. Both shader dialects use only line comments.
fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every screaming-snake constant a shader declares: the slot names, the
/// iteration counts, the mathematics, each scene's fixed identity. Extracted
/// rather than listed, so a constant added to one backend and forgotten in the
/// other is caught without anyone having to remember this file.
fn shader_constants(source: &str) -> BTreeSet<String> {
    strip_line_comments(source)
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|token| {
            token.len() >= SHORTEST_CONSTANT
                && token.starts_with(|c: char| c.is_ascii_uppercase())
                && token
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn the_hlsl_library_declares_every_constant_the_metal_one_does() {
    let graphics = repo_root().join("examples/graphics");
    let metal = read_required(&graphics.join("base.metal"));
    let hlsl = read_required(&graphics.join("base.hlsl"));

    // base.metal is the reference: every constant it names has to exist in the
    // HLSL twin, or the two backends have started rendering different pictures.
    // The reverse is not required — HLSL needs a register count Metal does not.
    let missing: Vec<String> = shader_constants(&metal)
        .difference(&shader_constants(&hlsl))
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "base.hlsl is missing constants base.metal declares: {missing:?}"
    );

    // Folding log2(e) by hand — the reason moving the falloff rate into Osprey
    // changed no pixel — has to survive the crossing, and both dialects spell
    // the constant the same way so it reads as one helper in two files.
    assert!(
        hlsl.contains("exp2(-rate * M_LOG2E_F * d)"),
        "base.hlsl's falloff helper must fold log2(e) by hand like base.metal's"
    );
}

#[test]
fn the_hlsl_constant_buffer_cannot_quietly_unpack_itself() {
    let hlsl = read_required(&repo_root().join("examples/graphics/base.hlsl"));

    // The silent corruption this backend is most exposed to. A `float[24]` in
    // an HLSL constant buffer takes one 16-byte register PER ELEMENT, so it
    // would span 384 bytes and read `slot[1]` out of the bridge's `slot[4]` —
    // and nothing about that fails to compile. Packing it as float4 is the fix;
    // the register count must be derived, or the two can drift apart.
    assert!(
        hlsl.contains("#define OSP_GFX_SLOT_REGISTERS (OSP_GFX_SLOTS / 4)")
            && hlsl.contains("float4 uniformSlotPack[OSP_GFX_SLOT_REGISTERS];"),
        "base.hlsl must pack the slot array as float4 sized from OSP_GFX_SLOTS"
    );
    assert_eq!(
        hlsl.matches("cbuffer OspGfxUniforms : register(b0)")
            .count(),
        1,
        "base.hlsl must bind exactly one constant buffer at b0"
    );

    // That argument holds only for a whole number of float4 registers, and a
    // shader compiler will not say so — the guard in the file has to.
    assert!(
        hlsl.contains("#if (OSP_GFX_SLOTS % 4) != 0") && hlsl.contains("#error"),
        "base.hlsl must refuse to compile if the slot count stops packing evenly"
    );
}

#[test]
fn every_declaration_of_the_uniform_layout_agrees() {
    let graphics = repo_root().join("examples/graphics");
    let host = read_required(&graphics.join("base/base.osp"));
    let metal_bridge = read_required(&graphics.join("ospgfx.m"));
    // The Windows bridge is three files — the shared ABI header, the one-time
    // bring-up and the exported entry points — and it is the union of them that
    // has to match ospgfx.m, so it is compared as one text.
    let d3d_abi = read_required(&graphics.join("ospgfx_d3d12.c"));
    let d3d = [
        read_required(&graphics.join("ospgfx_d3d12.h")),
        read_required(&graphics.join("ospgfx_d3d12_setup.c")),
        d3d_abi.clone(),
    ]
    .join("\n");

    // This is the silent-corruption bug the shared-library layout exists to
    // prevent, now doubled: four independent declarations of one layout — two
    // bridges and two shader libraries — and no compiler can check any of them
    // against the others. A disagreement compiles cleanly and then shifts `res`
    // and `time` under every scene on one of the two platforms.
    let slots = number_after(&metal_bridge, "#define OSP_GFX_SLOTS");
    for (name, source, res) in [
        ("ospgfx.m", &metal_bridge, "float res[2];"),
        (
            "base.metal",
            &read_required(&graphics.join("base.metal")),
            "float2 res;",
        ),
        ("the D3D12 bridge", &d3d, "float res[2];"),
        (
            "base.hlsl",
            &read_required(&graphics.join("base.hlsl")),
            "float2 res;",
        ),
    ] {
        assert_eq!(
            number_after(source, "#define OSP_GFX_SLOTS"),
            slots,
            "OSP_GFX_SLOTS in {name} disagrees with ospgfx.m"
        );
        assert!(
            source.contains("float slot[OSP_GFX_SLOTS];")
                && source.contains(res)
                && source.contains("float time;")
                && source.contains("float pad;"),
            "{name}'s uniform layout lost a field or stopped sizing itself from OSP_GFX_SLOTS"
        );
    }
    assert!(
        slots.unwrap_or_default() > LOOK_SLOT_LAST,
        "OSP_GFX_SLOTS is too small for look slot {LOOK_SLOT_LAST}"
    );

    // The host's own view of the same layout: where the grade starts and ends.
    assert!(
        host.contains("fn motionCount() = 8")
            && host.contains("fn lookFirst() = 8")
            && host.contains(&format!("fn lookLast() = {LOOK_SLOT_LAST}")),
        "the shared host must agree with the shaders about the slot ranges"
    );

    // Osprey's `extern fn` declarations are written once and linked against
    // whichever bridge the platform built, so the two present one C ABI: the
    // same six symbols, both slot scales — motion in fixed point, the grade in
    // thousandths, because the grade's exactness depends on that division — a
    // fragment entry as a parameter, and a diagnostic that names the entry it
    // could not find. A symbol missing from one side is a link error there and
    // nothing at all on the other.
    for (name, bridge) in [("ospgfx.m", &metal_bridge), ("the D3D12 bridge", &d3d)] {
        for export in BRIDGE_EXPORTS {
            assert!(
                bridge.contains(&format!("{export}(")),
                "{name} must keep exporting {export}"
            );
        }
        for shared in [
            "#define OSP_GFX_FIXED_ONE 4096.0f",
            "#define OSP_GFX_MILLI_ONE 1000.0f",
            "const char *fragmentName)",
            r#""ospgfx: shader needs osp_vertex and %s\n", entry"#,
        ] {
            assert!(bridge.contains(shared), "{name} must keep `{shared}`");
        }
    }
    for export in BRIDGE_EXPORTS {
        assert!(
            d3d_abi.contains(&format!("OSP_GFX_API int64_t {export}("))
                || d3d_abi.contains(&format!("OSP_GFX_API void *{export}(")),
            "the D3D12 bridge must mark {export} for export from its DLL"
        );
    }

    // The scenes name one shader path for every platform; the extension swap in
    // the Windows bridge is what keeps that true without editing a scene.
    assert!(
        d3d.contains(r#"#define OSP_GFX_SHADER_EXT ".hlsl""#)
            && host.contains(r#"fn shaderPath() = "examples/graphics/base.metal""#),
        "the host must name one shader path and the D3D12 bridge must resolve it"
    );
}
