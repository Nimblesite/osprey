//! Regression coverage for the shared real-time graphics examples.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn read_required(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("required graphics source {}: {error}", path.display()))
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

#[test]
fn graphics_scenes_share_two_bases_without_changing_the_original_mode() {
    let graphics = repo_root().join("examples/graphics");
    let host = read_required(&graphics.join("base.osp"));
    let shader = read_required(&graphics.join("base.metal"));

    for (caller, entry) in [
        ("scene.osp", "SceneBase::original()"),
        ("scene2.osp", "SceneBase::opal()"),
        ("scene3.osp", "SceneBase::character()"),
    ] {
        let source = read_required(&graphics.join(caller));
        assert!(
            source.contains("import graphics::SceneBase"),
            "{caller} must import the shared host"
        );
        assert!(source.contains(entry), "{caller} must call {entry}");
        assert!(
            source_lines(&source) <= 4,
            "{caller} must stay a thin entry point, got {} source lines",
            source_lines(&source)
        );
    }

    assert!(
        !graphics.join("scene2.metal").exists(),
        "scene2 must use base.metal instead of duplicating a shader"
    );
    assert!(
        !graphics.join("scene3.metal").exists(),
        "scene3 must use base.metal instead of duplicating a shader"
    );

    // These are the committed Kali timings. The shared host may add other
    // profiles, but original mode must continue to use these exact values.
    for exact_original in [
        "fn originalFoldShapeX(t) = swing(t, 27000, 4055, 1105)",
        "fn originalFoldShapeY(t) = sway(t, 37000, 3768, 1064)",
        "fn originalZoomLevel(t) = swing(t, 41000, 4300, 1350)",
        "fn originalSpinTurns(t) = phase(t, 73000)",
        "fn originalPaletteTurns(t) = phase(t, 19000)",
        "fn originalCoreGlow(t) = swing(t, 11000, 1150, 720)",
        "fn originalDriftX(t) = swing(t, 53000, 0, 700)",
        "fn originalDriftY(t) = sway(t, 61000, 0, 620)",
    ] {
        assert!(
            host.contains(exact_original),
            "original host behaviour changed or is missing: {exact_original}"
        );
    }

    assert!(
        host.contains("fn opalTime(t) = (t * 4) ?: t"),
        "opal mode must run at the requested faster animation pace"
    );

    // Pin the original shader's iteration count, palette, orbit trap, colour
    // treatment, vignette, tonemap, and gamma. Other named fragment entry
    // points can share this Metal library without perturbing original mode.
    for exact_original in [
        "fragment float4 osp_fragment(",
        "for (int i = 0; i < 18; i++)",
        "palette(0.085 * float(i) + 0.45 * d + u.slot[4]) * exp(-3.6 * d)",
        "0.006 + 3.0 * trap * trap",
        "col, 1.45)",
        "col *= 1.0 - 0.30 * dot(uv, uv)",
        "col = col / (1.0 + col)",
        "float3(0.72)",
    ] {
        assert!(
            shader.contains(exact_original),
            "original Metal behaviour changed or is missing: {exact_original}"
        );
    }

    for fragment in [
        "osp_fragment",
        "osp_fragment_opal",
        "osp_fragment_character",
    ] {
        assert!(
            shader.contains(fragment),
            "base.metal must expose named fragment entry {fragment}"
        );
    }
}
