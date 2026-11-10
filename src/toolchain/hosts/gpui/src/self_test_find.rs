//! Finding what a self-test step names ([UI-MACHINE-SELF-TEST]): a rendered node by the words a
//! screen reader announces for it, the control or chooser it belongs to, the first scene, and the
//! events a found node answers with.

use serde_json::{json, Value};

use super::{Attempt, Send};
use crate::regions::Regions;
use crate::smoke;

/// A rendered node a step found by its words: its region, its binding and the node.
pub(super) struct Found<'a> {
    region: String,
    ordinal: u64,
    node: &'a Value,
}

impl Found<'_> {
    pub(super) fn sent(&self, value: Value) -> Attempt {
        Attempt::Sending(vec![Send {
            region: self.region.clone(),
            ordinal: self.ordinal,
            value,
        }])
    }

    fn number(&self, key: &str) -> f64 {
        self.node.get(key).and_then(Value::as_f64).unwrap_or(0.)
    }

    fn pointer(&self, x: f64, y: f64, down: bool) -> Send {
        Send {
            region: self.region.clone(),
            ordinal: self.ordinal,
            value: json!({"x": x, "y": y, "down": down, "shift": false}),
        }
    }

    fn middle(&self) -> (f64, f64) {
        (
            self.number("x") + self.number("w") / 2.,
            self.number("y") + self.number("h") / 2.,
        )
    }

    /// The press and release a click on an accessible element lands as.
    pub(super) fn clicked(&self) -> Attempt {
        let (x, y) = self.middle();
        Attempt::Sending(vec![self.pointer(x, y, true), self.pointer(x, y, false)])
    }

    /// The press and release that set a slider to `value`, clamped to its range.
    pub(super) fn valued(&self, value: f64) -> Result<Attempt, String> {
        let (low, high) = (self.number("min"), self.number("max"));
        if high <= low {
            return Err(format!("{} has no range to set", self.node));
        }
        let fraction = ((value - low) / (high - low)).clamp(0., 1.);
        let (x, y) = (
            self.number("x") + self.number("w") * fraction,
            self.middle().1,
        );
        Ok(Attempt::Sending(vec![
            self.pointer(x, y, true),
            self.pointer(x, y, false),
        ]))
    }

    /// The press, move and release that drag an element by `dx`, `dy` scene units.
    pub(super) fn dragged(&self, dx: f64, dy: f64) -> Attempt {
        let (x, y) = self.middle();
        Attempt::Sending(vec![
            self.pointer(x, y, true),
            self.pointer(x + dx, y + dy, true),
            self.pointer(x + dx, y + dy, false),
        ])
    }
}

/// Every string in every region: a text, a label, a title or a scene's lettering.
pub(super) fn shows(regions: &Regions, words: &str) -> bool {
    regions.keys().any(|key| {
        regions
            .get(key)
            .is_some_and(|region| holds(&region.node, words))
    })
}

fn holds(node: &Value, words: &str) -> bool {
    node.as_str() == Some(words) || smoke::children(node).any(|inside| holds(inside, words))
}

/// The first scene and its binding and accessibility tree.
pub(super) fn scene(regions: &Regions) -> Option<(String, u64, &Value)> {
    regions.keys().find_map(|key| {
        let node = find(&regions.get(key)?.node, &|node| {
            node.get("kind").and_then(Value::as_str) == Some("scene")
        })?;
        Some((key.to_owned(), node.get("event")?.as_u64()?, node))
    })
}

fn find<'a>(node: &'a Value, wanted: &dyn Fn(&Value) -> bool) -> Option<&'a Value> {
    if wanted(node) {
        return Some(node);
    }
    smoke::children(node).find_map(|inside| find(inside, wanted))
}

/// The scene element a screen reader announces as `label`.
pub(super) fn element<'a>(regions: &'a Regions, label: &str) -> Option<Found<'a>> {
    let (region, ordinal, node) = scene(regions)?;
    let item = node
        .get("access")?
        .as_array()?
        .iter()
        .find(|item| item.get("label").and_then(Value::as_str) == Some(label))?;
    Some(Found {
        region,
        ordinal,
        node: item,
    })
}

/// The bound control a screen reader announces as `words`.
pub(super) fn control<'a>(regions: &'a Regions, words: &str) -> Option<Found<'a>> {
    let named = |node: &Value| {
        node.get("event").is_some_and(Value::is_u64)
            && ["label", "text", "title"]
                .iter()
                .any(|key| node.get(*key).and_then(Value::as_str) == Some(words))
    };
    regions.keys().find_map(|key| {
        let node = find(&regions.get(key)?.node, &named)?;
        Some(Found {
            region: key.to_owned(),
            ordinal: node.get("event")?.as_u64()?,
            node,
        })
    })
}

/// The choice a bound list or picker offers under the words on its row.
pub(super) fn chooser(regions: &Regions, words: &str) -> Option<Send> {
    regions.keys().find_map(|key| {
        let offers = |node: &Value, field: &str| {
            node.get(field)?.as_array()?.iter().find_map(|row| {
                let row_key = row.get("key")?.as_str()?;
                holds(row, words).then(|| row_key.to_owned())
            })
        };
        let node = find(&regions.get(key)?.node, &|node| {
            node.get("event").is_some()
                && (offers(node, "rows").is_some() || offers(node, "choices").is_some())
        })?;
        Some(Send {
            region: key.to_owned(),
            ordinal: node.get("event")?.as_u64()?,
            value: Value::String(offers(node, "rows").or_else(|| offers(node, "choices"))?),
        })
    })
}
