//! Regions by key, patched from each envelope ([GPUI-HOST-RENDER]): a host
//! keeps the opaque session, replaces a patched region's tree, drops a removed
//! one and holds the theme and the slices the framework wrote.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::envelope::{Envelope, Patch, Request, Slice, Theme};
use crate::ffi;
use crate::frame::{self, Cursor, Run};

/// One region's current tree and the revision it was drawn at.
#[derive(Debug, Clone)]
pub struct Region {
    pub revision: u64,
    pub node: Value,
    /// The shapes each streamed scene of this revision received, by the `streamSlot` the host gave
    /// the scene node ([UI-SCENE-STREAM]).
    pub streams: Vec<Run>,
}

/// Every region the framework has patched, in key order, plus the session,
/// theme and diagnostics the last envelopes reported.
#[derive(Debug, Clone, Default)]
pub struct Regions {
    regions: BTreeMap<String, Region>,
    state: String,
    theme: Option<Theme>,
    diagnostics: Vec<String>,
}

impl Regions {
    /// An empty host: no region, no session and no theme, as before `start`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies one envelope and returns the capabilities it asks the host to answer.
    pub fn apply(&mut self, envelope: Envelope) -> Result<Vec<Request>, String> {
        let stream = Cursor::new(frame::take());
        self.state = envelope.state;
        for key in envelope.removed {
            self.regions.remove(&key);
        }
        self.patch_regions(envelope.patches, stream);
        self.publish_slices(envelope.slices);
        if envelope.theme.is_some() {
            self.theme = envelope.theme;
        }
        self.diagnostics.extend(envelope.diagnostics);
        Ok(envelope.requests)
    }

    fn patch_regions(&mut self, patches: Vec<Patch>, mut stream: Cursor) {
        for patch in patches {
            self.patch_region(patch, &mut stream);
        }
        if stream.remaining() > 0 {
            self.diagnostics
                .push("the application streamed more shapes than its scenes counted".to_owned());
        }
    }

    fn patch_region(&mut self, patch: Patch, stream: &mut Cursor) {
        let mut node = patch.node;
        let mut streams = Vec::new();
        if let Err(problem) = attach(&mut node, stream, &mut streams) {
            self.diagnostics.push(problem);
        }
        let _ = self.regions.insert(
            patch.key,
            Region {
                revision: patch.revision,
                node,
                streams,
            },
        );
    }

    fn publish_slices(&mut self, slices: Vec<Slice>) {
        for slice in slices {
            if let Err(problem) = ffi::put_slice(&slice.key, &slice.text) {
                self.diagnostics.push(problem);
            }
        }
    }

    /// The region drawn under `key`, if the framework has patched it.
    pub fn get(&self, key: &str) -> Option<&Region> {
        self.regions.get(key)
    }

    /// Every held region key, in order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.regions.keys().map(String::as_str)
    }

    /// The opaque session string, returned unchanged with the next event.
    pub fn state(&self) -> &str {
        &self.state
    }

    /// The theme the start envelope carried; later envelopes leave it alone.
    pub fn theme(&self) -> Option<&Theme> {
        self.theme.as_ref()
    }

    /// Every diagnostic reported since the session started.
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
}

/// Gives each streamed scene in `node` its run of the call's stream, walking children in the order
/// the framework's encoder walks them, which is the order it streamed them in ([UI-SCENE-STREAM]).
fn attach(node: &mut Value, stream: &mut Cursor, streams: &mut Vec<Run>) -> Result<(), String> {
    let kind = node.get("kind").and_then(Value::as_str).unwrap_or_default();
    if kind == "scene" {
        let count = node.get("streamed").and_then(Value::as_u64).unwrap_or(0);
        if count == 0 {
            return Ok(());
        }
        let wanted = usize::try_from(count).unwrap_or(usize::MAX);
        let run = stream.run(wanted).map_err(|sent| {
            format!("a scene counted {count} streamed shapes but the application sent {sent}")
        })?;
        if let Some(fields) = node.as_object_mut() {
            fields.insert("streamSlot".to_owned(), Value::from(streams.len()));
        }
        streams.push(run);
        return Ok(());
    }
    let order: &[&str] = match kind {
        "screen" => &["toolbar", "children"],
        "aside" => &["pane", "aside"],
        "context-menu" => &["body", "items"],
        "tabs" => &["tabs"],
        "rows" => &["rows"],
        _ => &["children"],
    };
    for member in order {
        match node.get_mut(*member) {
            Some(Value::Array(items)) => {
                for item in items {
                    // Tabs and rows hold their content under `child`.
                    let inner = if item.get("kind").is_some() {
                        item
                    } else if let Some(child) = item.get_mut("child") {
                        child
                    } else {
                        continue;
                    };
                    attach(inner, stream, streams)?;
                }
            }
            Some(single @ Value::Object(_)) => attach(single, stream, streams)?,
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{attach, Cursor};
    use crate::ffi::test_support::serial;
    use crate::frame::{self, Streamed};
    use serde_json::Value;

    /// Streams discs at depths `1..=count` through the real bridge and seals them as one frame.
    fn dealt(count: i64) -> Cursor {
        for depth in 1..=count {
            assert!(frame::stream(Streamed {
                code: 1,
                index: 0,
                values: [0.; 5],
                colour: 0,
                depth,
            }));
        }
        frame::seal().expect("a sealed frame");
        Cursor::new(frame::take())
    }

    /// Scenes take their runs in the encoder's order: a screen's toolbar before its children and an
    /// aside's pane before its column, whatever order the JSON members sort in ([UI-SCENE-STREAM]).
    #[test]
    fn streamed_runs_follow_the_encoder_walk() {
        let _serial = serial();
        let mut node: Value = serde_json::from_str(r#"{"kind":"screen","children":[{"kind":"aside","aside":{"kind":"scene","streamed":1},"pane":{"kind":"scene","streamed":2}}],"toolbar":[{"kind":"scene","streamed":1}]}"#).unwrap();
        let mut stream = dealt(4);
        let mut streams = Vec::new();
        attach(&mut node, &mut stream, &mut streams).unwrap();
        assert_eq!(stream.remaining(), 0);
        drop(stream);
        let depths: Vec<Vec<i64>> = streams
            .iter()
            .map(|run| run.iter().map(|item| item.depth).collect())
            .collect();
        assert_eq!(depths, vec![vec![1], vec![2, 3], vec![4]]);
        assert_eq!(node["children"][0]["pane"]["streamSlot"], 1);
        assert_eq!(node["children"][0]["aside"]["streamSlot"], 2);
        // A later frame replaces the published one; the runs still read the frame they lease.
        let replaced = dealt(9);
        assert_eq!(replaced.remaining(), 9);
        let held: Vec<i64> = streams
            .iter()
            .flat_map(|run| run.iter())
            .map(|item| item.depth)
            .collect();
        assert_eq!(held, vec![1, 2, 3, 4]);
    }

    /// A scene that counts more shapes than arrived is a diagnostic, never a partial draw.
    #[test]
    fn a_short_stream_is_reported() {
        let _serial = serial();
        let mut node: Value =
            serde_json::from_str(r#"{"kind":"stack","children":[{"kind":"scene","streamed":3}]}"#)
                .unwrap();
        let mut stream = dealt(2);
        let problem = attach(&mut node, &mut stream, &mut Vec::new()).unwrap_err();
        assert!(
            problem.contains("counted 3") && problem.contains("sent 2"),
            "{problem}"
        );
        assert_eq!(stream.remaining(), 0);
    }

    /// An envelope applied without a sealed frame deals no shapes and still commits its session;
    /// shapes left over after every scene took its run are reported.
    #[test]
    fn an_envelope_takes_exactly_the_frame_its_export_sealed() {
        let _serial = serial();
        let mut regions = super::Regions::new();
        drop(frame::take());
        assert!(regions.apply(session("before")).is_ok());
        assert_eq!(regions.state(), "before");
        assert!(regions.diagnostics().is_empty());
        drop(dealt(2));
        assert!(frame::stream(Streamed {
            code: 0,
            index: 0,
            values: [0.; 5],
            colour: 0,
            depth: 0
        }));
        frame::seal().expect("a sealed frame");
        assert!(regions.apply(session("after")).is_ok());
        assert_eq!(regions.state(), "after");
        assert_eq!(
            regions.diagnostics().len(),
            1,
            "{:?}",
            regions.diagnostics()
        );
        assert!(frame::take().is_none(), "the frame was handed over twice");
    }

    fn session(state: &str) -> crate::envelope::Envelope {
        crate::envelope::parse(&format!(
            r#"{{"version":1,"state":"{state}","patches":[],"requests":[]}}"#
        ))
        .expect("valid envelope")
    }
}
