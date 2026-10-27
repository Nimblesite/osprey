//! Regions by key, patched from each envelope ([GPUI-HOST-RENDER]): a host
//! keeps the opaque session, replaces a patched region's tree, drops a removed
//! one and holds the theme and the slices the framework wrote.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::envelope::{Envelope, Request, Theme};
use crate::ffi;

/// One region's current tree and the revision it was drawn at.
#[derive(Debug, Clone)]
pub struct Region {
    pub revision: u64,
    pub node: Value,
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
        for patch in patches {
            self.regions.insert(
                patch.key,
                Region {
                    revision: patch.revision,
                    node: patch.node,
                },
            );
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
