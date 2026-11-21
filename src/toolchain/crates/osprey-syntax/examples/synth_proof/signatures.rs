type Signature = (&'static str, &'static [&'static str], &'static str);

pub(super) fn signatures(module: &str) -> Result<&'static [Signature], String> {
    Ok(match module {
        "CanvasGeometry" => &[("offset", &["α", "α"], "α"), ("shifted", &["α", "α"], "α")],
        "DialMath" => &[("moved", &["α", "α"], "α")],
        "FitMath" => &[("clip", &["α", "α", "α"], "α")],
        "TimelineGeometry" => &[("heardSeconds", &["α", "α", "α", "α", "α"], "α")],
        "DelayLines" => &[("push", &["DelayLine α", "α"], "DelayLine α")],
        "ArrangeMath" => &[
            ("gated", &["α", "α", "α", "α"], "α"),
            ("enveloped", &["α", "α", "α", "α", "α", "α"], "α"),
        ],
        "PhaseMath" => &[
            ("window", &["α"], "α"),
            ("windowedPhase", &["α", "α", "α"], "α"),
        ],
        "PhaseOperators" => &[
            ("basePhase", &["α", "α"], "α"),
            ("pulse", &["α", "Pulse α"], "α"),
            ("shaped", &["PhaseShape α", "α"], "α"),
        ],
        "GrainMath" => &[("asr", &["α", "α", "α", "α"], "α")],
        "GrainWindows" => &[
            ("hann", &["α", "α"], "α"),
            ("value", &["Envelope α", "Nat", "Nat"], "α"),
        ],
        "Interpolation" => &[("linear", &["α", "α", "α"], "α")],
        "Waveshapers" => &[
            ("curve", &["Curve", "α"], "α"),
            ("integral", &["Curve", "α"], "α"),
            ("shaped", &["Shaper", "α", "α"], "α"),
        ],
        "Allpasses" => &[("next", &["α", "AllpassState α", "α"], "AllpassSample α")],
        "StrikeCombs" => &[
            ("delay", &["Strike α"], "α"),
            (
                "next",
                &["Strike α", "DelayLine α", "α"],
                "Except ops.Fault (LineSample α (DelayLine α))",
            ),
        ],
        "SpectralStreams" => &[
            ("overlapScale", &["α", "α"], "α"),
            ("overlapped", &["α", "α", "α"], "α"),
        ],
        "TableMath" => &[("harmonic", &["α", "α", "α", "α"], "α")],
        "Wavetables" => &[("value", &["List (Harmonic α)", "α", "α"], "α")],
        "SpectralMath" => &[
            ("hann", &["α", "α"], "α"),
            ("twiddle", &["α", "α", "α"], "α"),
            ("productRe", &["α", "α", "α", "α"], "α"),
            ("productIm", &["α", "α", "α", "α"], "α"),
        ],
        "SpectralFrames" => &[
            ("scaled", &["Bin α", "α"], "Bin α"),
            ("turned", &["Bin α", "Bin α"], "Bin α"),
            ("rotation", &["Bin α", "Bin α"], "Bin α"),
        ],
        "Ffts" => &[
            ("incomplete", &[], "Except ops.Fault (List (Bin α))"),
            ("conjugate", &["Bin α"], "Bin α"),
            ("added", &["Bin α", "Bin α"], "Bin α"),
            ("subtracted", &["Bin α", "Bin α"], "Bin α"),
            ("rotated", &["Bin α", "α"], "Bin α"),
            (
                "quartet",
                &["Bin α", "Bin α", "Bin α", "Bin α", "α"],
                "List (Bin α)",
            ),
            (
                "four",
                &["List (Bin α)", "Nat", "Nat", "α"],
                "Except ops.Fault (List (Bin α))",
            ),
            (
                "combined",
                &["List (Bin α)", "List (Bin α)", "Nat", "Nat", "α"],
                "Halves α",
            ),
            (
                "transform",
                &["List (Bin α)", "Nat", "Nat", "Nat", "α"],
                "Except ops.Fault (List (Bin α))",
            ),
        ],
        other => return Err(format!("unsupported production module {other}")),
    })
}
