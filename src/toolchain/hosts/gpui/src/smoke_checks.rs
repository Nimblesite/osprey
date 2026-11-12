//! What a smoke step expects of the host after it runs ([GPUI-HOST-VERIFICATION]): the envelope's
//! regions and requests, the rendered text and the diagnostics count.

use super::{has_text, Expect, TextExpect};
use crate::envelope::Envelope;
use crate::regions::Regions;

/// Checks patch membership and the ordered capability request list.
pub(super) fn check_envelope(envelope: &Envelope, expect: &Expect) -> Result<(), String> {
    if let Some(expected) = &expect.state {
        if &envelope.state != expected {
            return Err(format!(
                "session state was {:?}, expected {expected:?}",
                envelope.state
            ));
        }
    }
    let patches: Vec<&str> = envelope
        .patches
        .iter()
        .map(|patch| patch.key.as_str())
        .collect();
    if let Some(expected) = &expect.patched {
        for key in expected {
            if !patches.contains(&key.as_str()) {
                return Err(format!(
                    "region {key} was not patched; patches were {patches:?}"
                ));
            }
        }
    }
    check_requests(envelope, expect.requests.as_deref())
}

/// Checks the exact ordered request list when a step supplies one.
fn check_requests(envelope: &Envelope, expected: Option<&[String]>) -> Result<(), String> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual: Vec<&str> = envelope
        .requests
        .iter()
        .map(|request| request.kind.as_str())
        .collect();
    let wanted: Vec<&str> = expected.iter().map(String::as_str).collect();
    match actual == wanted {
        true => Ok(()),
        false => Err(format!("requests were {actual:?}, expected {wanted:?}")),
    }
}

/// Checks current region existence, text and accumulated diagnostics.
pub(super) fn check_regions(regions: &Regions, expect: &Expect) -> Result<(), String> {
    if let Some(expected) = &expect.regions {
        for key in expected {
            if regions.get(key).is_none() {
                let keys: Vec<&str> = regions.keys().collect();
                return Err(format!("region {key} is absent; regions were {keys:?}"));
            }
        }
    }
    if let Some(expected) = &expect.primitives {
        for (key, count) in expected {
            check_primitives(regions, key, *count)?;
        }
    }
    check_text(regions, expect.text.as_ref())?;
    check_diagnostics(regions, expect.diagnostics)
}

/// Count both transports so moving a primitive out of JSON cannot hide its loss.
fn check_primitives(regions: &Regions, key: &str, expected: usize) -> Result<(), String> {
    let region = regions
        .get(key)
        .ok_or_else(|| format!("scene region {key} is absent"))?;
    let shapes = region
        .node
        .get("shapes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("region {key} has no scene primitives"))?;
    let actual = shapes.len()
        + region
            .streams
            .iter()
            .map(|stream| stream.len())
            .sum::<usize>();
    if actual != expected {
        return Err(format!(
            "scene {key} has {actual} primitives, expected {expected}"
        ));
    }
    Ok(())
}

/// Checks one substring against all strings in the selected current region.
fn check_text(regions: &Regions, expect: Option<&TextExpect>) -> Result<(), String> {
    let Some(expect) = expect else {
        return Ok(());
    };
    let region = regions
        .get(&expect.region)
        .ok_or_else(|| format!("region {} is absent for a text assertion", expect.region))?;
    match has_text(&region.node, &expect.contains, true) {
        true => Ok(()),
        false => Err(format!(
            "region {} has no string containing {:?}: {}",
            expect.region, expect.contains, region.node
        )),
    }
}

/// Checks the exact accumulated diagnostic count when a step supplies one.
fn check_diagnostics(regions: &Regions, expected: Option<usize>) -> Result<(), String> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = regions.diagnostics();
    match actual.len() == expected {
        true => Ok(()),
        false => Err(format!(
            "diagnostics were {actual:?}, expected {expected} entries"
        )),
    }
}
