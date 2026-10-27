//! Regions by key, patched from each envelope ([GPUI-HOST-RENDER]): a host
//! keeps the opaque session, replaces a patched region's tree, drops a removed
//! one and holds the theme and the slices the framework wrote.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use crate::envelope::{Envelope, Request, Theme};
use crate::ffi::{self, Streamed};

/// One region's current tree and the revision it was drawn at.
#[derive(Debug, Clone)]
pub struct Region {
    pub revision: u64,
    pub node: Value,
    /// The shapes each streamed scene of this revision received, by the `streamSlot` the host gave
    /// the scene node ([UI-SCENE-STREAM]).
    pub streams: Vec<Arc<Vec<Streamed>>>,
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
    pub fn apply(&mut self, envelope: Envelope) -> Vec<Request> {
        let Envelope {
            state,
            slices,
            patches,
            removed,
            requests,
            diagnostics,
            theme,
            ..
        } = envelope;
        self.state = state;
        for key in removed {
            self.regions.remove(&key);
        }
        let mut stream = ffi::take_stream().into_iter();
        for patch in patches {
            let mut node = patch.node;
            let mut streams = Vec::new();
            if let Err(problem) = attach(&mut node, &mut stream, &mut streams) {
                self.diagnostics.push(problem);
            }
            self.regions.insert(
                patch.key,
                Region {
                    revision: patch.revision,
                    node,
                    streams,
                },
            );
        }
        if stream.next().is_some() {
            self.diagnostics
                .push("the application streamed more shapes than its scenes counted".to_owned());
        }
        for slice in slices {
            if let Err(problem) = ffi::put_slice(&slice.key, &slice.text) {
                self.diagnostics.push(problem);
            }
        }
        if theme.is_some() {
            self.theme = theme;
        }
        self.diagnostics.extend(diagnostics);
        requests
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
fn attach(
    node: &mut Value,
    stream: &mut impl Iterator<Item = Streamed>,
    streams: &mut Vec<Arc<Vec<Streamed>>>,
) -> Result<(), String> {
    let kind = node.get("kind").and_then(Value::as_str).unwrap_or_default();
    if kind == "scene" {
        let count = node.get("streamed").and_then(Value::as_u64).unwrap_or(0);
        if count == 0 {
            return Ok(());
        }
        let run: Vec<Streamed> = stream.take(count as usize).collect();
        if run.len() as u64 != count {
            return Err(format!(
                "a scene counted {count} streamed shapes but the application sent {}",
                run.len()
            ));
        }
        if let Some(fields) = node.as_object_mut() {
            fields.insert("streamSlot".to_owned(), Value::from(streams.len()));
        }
        streams.push(Arc::new(run));
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
    use super::{attach, Streamed};
    use serde_json::Value;

    fn shape(depth: i64) -> Streamed {
        Streamed {
            code: 1,
            index: 0,
            values: [0.; 5],
            colour: 0,
            depth,
        }
    }

    /// Scenes take their runs in the encoder's order: a screen's toolbar before its children and an
    /// aside's pane before its column, whatever order the JSON members sort in ([UI-SCENE-STREAM]).
    #[test]
    fn streamed_runs_follow_the_encoder_walk() {
        let mut node: Value = serde_json::from_str(r#"{"kind":"screen","children":[{"kind":"aside","aside":{"kind":"scene","streamed":1},"pane":{"kind":"scene","streamed":2}}],"toolbar":[{"kind":"scene","streamed":1}]}"#).unwrap();
        let mut stream = (1..=4).map(shape);
        let mut streams = Vec::new();
        attach(&mut node, &mut stream, &mut streams).unwrap();
        let depths: Vec<Vec<i64>> = streams
            .iter()
            .map(|run| run.iter().map(|item| item.depth).collect())
            .collect();
        assert_eq!(depths, vec![vec![1], vec![2, 3], vec![4]]);
        assert_eq!(node["children"][0]["pane"]["streamSlot"], 1);
        assert_eq!(node["children"][0]["aside"]["streamSlot"], 2);
    }

    /// A scene that counts more shapes than arrived is a diagnostic, never a partial draw.
    #[test]
    fn a_short_stream_is_reported() {
        let mut node: Value =
            serde_json::from_str(r#"{"kind":"stack","children":[{"kind":"scene","streamed":3}]}"#)
                .unwrap();
        let mut stream = (1..=2).map(shape);
        assert!(attach(&mut node, &mut stream, &mut Vec::new()).is_err());
    }
}
