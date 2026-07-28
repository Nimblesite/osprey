//! Linkage-name encoding for namespaced declarations, and its inverse.
//!
//! Project assembly rewrites `bank::Api::Audit` to a flat, link-safe
//! `__osp_4x62616e6b_3x417069_5x4175646974` so two namespaces may declare the
//! same short name. That name is an implementation detail: nothing a user
//! reads — a diagnostic, a symbol listing, a hover — may show it. Encoding and
//! decoding therefore live together here, so the two can never drift.

use std::borrow::Cow;

/// Prefix every encoded symbol carries.
const PREFIX: &str = "__osp";
/// Separator between the byte length and the hex-encoded bytes of a segment.
const LENGTH_TERMINATOR: char = 'x';
/// Two hex characters encode one byte.
const HEX_PER_BYTE: usize = 2;
const HEX_RADIX: u32 = 16;

/// Encode namespace-and-path segments as one flat linkage name.
#[must_use]
pub fn mangle<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    segments
        .into_iter()
        .fold(String::from(PREFIX), |mut out, segment| {
            push_segment(&mut out, segment);
            out
        })
}

fn push_segment(out: &mut String, segment: &str) {
    use std::fmt::Write as _;
    let _ = write!(out, "_{}{LENGTH_TERMINATOR}", segment.len());
    for byte in segment.as_bytes() {
        let _ = write!(out, "{byte:02x}");
    }
}

/// Decode one whole linkage name back to its `a::b::c` source name.
///
/// Returns `None` when `symbol` is not a complete encoded name, so unrelated
/// runtime symbols such as `__osprey_handler_push` are left alone.
#[must_use]
pub fn demangle(symbol: &str) -> Option<String> {
    let (source_name, consumed) = decode_prefix(symbol)?;
    (consumed == symbol.len()).then_some(source_name)
}

/// Replace every encoded name embedded in `text` with its source name.
///
/// Text that contains none is returned borrowed, so this is free on the
/// overwhelmingly common path.
#[must_use]
pub fn demangle_message(text: &str) -> Cow<'_, str> {
    if !text.contains(PREFIX) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(PREFIX) {
        let (head, tail) = rest.split_at(start);
        out.push_str(head);
        // A `None` here is not an encoded name after all: emit the marker
        // literally and resume past it, so the scan cannot loop on it forever.
        let (decoded, consumed) =
            decode_prefix(tail).unwrap_or_else(|| (PREFIX.to_owned(), PREFIX.len()));
        out.push_str(&decoded);
        rest = tail.get(consumed..).unwrap_or_default();
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Decode the longest encoded name starting at `text`, returning the source
/// name and how many bytes it consumed.
fn decode_prefix(text: &str) -> Option<(String, usize)> {
    let mut rest = text.strip_prefix(PREFIX)?;
    let mut segments: Vec<String> = Vec::new();
    let mut consumed = PREFIX.len();
    while let Some((segment, tail)) = rest.strip_prefix('_').and_then(decode_segment) {
        consumed += rest.len() - tail.len();
        segments.push(segment);
        rest = tail;
    }
    let joined = segments
        .iter()
        .filter(|segment| !segment.is_empty())
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("::");
    (!segments.is_empty() && !joined.is_empty()).then_some((joined, consumed))
}

/// Decode one `<len>x<hex>` segment, returning it and the unconsumed tail.
fn decode_segment(text: &str) -> Option<(String, &str)> {
    let (length, hex) = text.split_once(LENGTH_TERMINATOR)?;
    let length: usize = length.parse().ok()?;
    let (bytes, tail) = hex.split_at_checked(length.checked_mul(HEX_PER_BYTE)?)?;
    let decoded = bytes
        .as_bytes()
        .chunks_exact(HEX_PER_BYTE)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|pair| u8::from_str_radix(pair, HEX_RADIX).ok())
        })
        .collect::<Option<Vec<u8>>>()?;
    String::from_utf8(decoded).ok().map(|text| (text, tail))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BANK_SERVE: &str = "__osp_4x62616e6b_5x7365727665";
    const BANK_AUDIT: &str = "__osp_4x62616e6b_3x417069_5x4175646974";

    #[test]
    fn every_mangled_name_round_trips_through_demangle() {
        for segments in [
            vec!["bank", "serve"],
            vec!["bank", "Api", "Audit"],
            vec!["main"],
            vec!["a", "b", "c", "d", "e"],
            vec!["Ünïcödé", "naïve"],
            vec!["with_underscore", "x1"],
        ] {
            let mangled = mangle(segments.iter().copied());
            assert_eq!(
                demangle(&mangled).as_deref(),
                Some(segments.join("::").as_str()),
                "{mangled}"
            );
        }
    }

    #[test]
    fn the_encoding_matches_the_names_project_assembly_emits() {
        assert_eq!(mangle(["bank", "serve"]), BANK_SERVE);
        assert_eq!(mangle(["bank", "Api", "Audit"]), BANK_AUDIT);
    }

    #[test]
    fn an_empty_namespace_segment_is_dropped_from_the_source_name() {
        assert_eq!(mangle(["", "main"]), "__osp_0x_4x6d61696e");
        assert_eq!(demangle("__osp_0x_4x6d61696e").as_deref(), Some("main"));
    }

    #[test]
    fn a_runtime_symbol_that_merely_starts_with_the_prefix_is_not_a_name() {
        for symbol in [
            "__osprey_handler_push",
            "__osp_cov_init",
            "__osp",
            "__osp_",
            "__osp_2xzz",
            "__osp_9x6d61696e",
            "osp_4x62616e6b",
        ] {
            assert_eq!(demangle(symbol), None, "{symbol}");
        }
    }

    #[test]
    fn a_trailing_operation_name_is_not_swallowed() {
        assert_eq!(demangle(&format!("{BANK_AUDIT}.log")), None);
        assert_eq!(
            demangle_message(&format!("{BANK_AUDIT}.log")),
            "bank::Api::Audit.log"
        );
    }

    #[test]
    fn a_diagnostic_has_every_embedded_name_replaced() {
        let message = format!(
            "function `{BANK_SERVE}` performs effects outside its declared row: {BANK_AUDIT}.log"
        );
        assert_eq!(
            demangle_message(&message),
            "function `bank::serve` performs effects outside its declared row: bank::Api::Audit.log"
        );
    }

    #[test]
    fn text_without_the_prefix_is_returned_borrowed() {
        assert!(matches!(
            demangle_message("unhandled effect operations at program entry: Alarm.ring"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn an_unparseable_prefix_is_emitted_literally_and_the_scan_continues() {
        assert_eq!(
            demangle_message(&format!(
                "declare void @__osprey_handler_pop() and {BANK_SERVE}"
            )),
            "declare void @__osprey_handler_pop() and bank::serve"
        );
    }
}
