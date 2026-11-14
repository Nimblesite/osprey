//! Regions by key, told by each frame ([GPUI-HOST-RENDER], [NATIVE-SHM-NODES]): a region keeps the
//! component tree the frame told it, a region whose tree did not change keeps the same tree rather
//! than being built again, and the theme a frame tells is held for the renderer. Nothing here is
//! application state: the session lives in the application's library.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::rows::Shown;
use crate::tree::{self, Frame, Theme, Tree};

/// One region's current tree.
#[derive(Debug, Clone)]
pub struct Region {
    pub node: Arc<Tree>,
}

/// Every region the last frame told, in key order, with its theme and the controls that deliver
/// events.
#[derive(Debug, Clone, Default)]
pub struct Regions {
    regions: BTreeMap<String, Region>,
    theme: Option<Theme>,
    /// Whether the last frame told components; a frame of plain view rows is drawn by the row view.
    told: bool,
    /// Every control number the told components issued, so an editor is kept while it is drawn.
    controls: BTreeSet<i64>,
}

impl Regions {
    /// An empty host: no region and no theme, as before the application's first frame.
    pub fn new() -> Self {
        Self {
            told: true,
            ..Self::default()
        }
    }

    /// Keeps the regions of one checked frame, refusing the frame whole when it draws something
    /// this host cannot ([UI-PROTOCOL]); the last good regions stay.
    pub fn adopt(&mut self, shown: &Shown) -> Result<(), String> {
        let Some(frame) = tree::frame(shown)? else {
            self.told = shown.controls.is_empty();
            self.controls = shown.controls.iter().filter_map(|row| row.action).collect();
            return Ok(());
        };
        self.told = true;
        let Frame {
            theme,
            regions: told,
        } = frame;
        if theme != self.theme {
            self.theme = theme;
        }
        let (regions, controls) = self.kept(told);
        self.regions = regions;
        self.controls = controls;
        Ok(())
    }

    /// The frame's regions, each reusing the tree it already held when it did not change, and every
    /// control number they issue.
    fn kept(&self, told: Vec<(String, Tree)>) -> (BTreeMap<String, Region>, BTreeSet<i64>) {
        let mut controls = BTreeSet::new();
        let regions = told
            .into_iter()
            .map(|(key, node)| {
                ordinals(&node, &mut controls);
                let node = match self.regions.get(&key) {
                    Some(unchanged) if *unchanged.node == node => unchanged.node.clone(),
                    _ => Arc::new(node),
                };
                (key, Region { node })
            })
            .collect();
        (regions, controls)
    }

    /// The region drawn under `key`, if the frame told it.
    pub fn get(&self, key: &str) -> Option<&Region> {
        self.regions.get(key)
    }

    /// The theme the frames told; later frames leave an unchanged one alone.
    pub fn theme(&self) -> Option<&Theme> {
        self.theme.as_ref()
    }

    /// Whether the last frame told components, rather than only plain view rows.
    pub fn told(&self) -> bool {
        self.told
    }

    /// Whether the frame issues a control event numbered `ordinal`.
    pub fn delivers(&self, ordinal: i64) -> bool {
        self.controls.contains(&ordinal)
    }
}

/// Collects every control number the told components issued: a control's event, an editor's place,
/// a board's lasso and pan.
fn ordinals(node: &Tree, into: &mut BTreeSet<i64>) {
    for name in ["event", "placeEvent", "lassoEvent", "panEvent"] {
        if let Some(number) = node.get(name).and_then(Tree::as_i64) {
            into.insert(number);
        }
    }
    for child in tree::inside(node) {
        ordinals(child, into);
    }
}

#[cfg(test)]
mod tests {
    use super::{ordinals, Frame, Regions, Tree};
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    /// A component with the members given.
    fn record(members: [(&str, Tree); 2]) -> Tree {
        Tree::Record(
            members
                .iter()
                .map(|(name, value)| ((*name).to_owned(), value.clone()))
                .collect(),
        )
    }

    fn frame(node: Tree) -> Frame {
        Frame {
            theme: None,
            regions: vec![("root".to_owned(), node)],
        }
    }

    /// A region whose tree did not change is left untouched; a changed one is replaced.
    #[test]
    fn a_region_whose_tree_did_not_change_keeps_its_tree() {
        let mut regions = Regions::new();
        let one = frame(record([
            ("kind", Tree::Text("stack".to_owned())),
            ("other", Tree::Whole(1)),
        ]));
        regions.regions = regions.kept(one.regions.clone()).0;
        let first = regions.get("root").expect("the root").node.clone();
        regions.regions = regions.kept(one.regions).0;
        assert!(Arc::ptr_eq(
            &first,
            &regions.get("root").expect("the root").node
        ));
        let two = frame(record([
            ("kind", Tree::Text("screen".to_owned())),
            ("other", Tree::Whole(1)),
        ]));
        regions.regions = regions.kept(two.regions).0;
        assert!(!Arc::ptr_eq(
            &first,
            &regions.get("root").expect("the root").node
        ));
    }

    /// Every control number the frame's components issue is kept, so an editor stays while drawn.
    #[test]
    fn every_control_number_the_tree_issues_is_kept() {
        let mut members = BTreeMap::new();
        members.insert("event".to_owned(), Tree::Whole(3));
        members.insert("placeEvent".to_owned(), Tree::Whole(4));
        members.insert("text".to_owned(), Tree::Text("not a control".to_owned()));
        let mut child = BTreeMap::new();
        child.insert("event".to_owned(), Tree::Whole(5));
        members.insert("body".to_owned(), Tree::Record(child));
        let mut kept = BTreeSet::new();
        ordinals(&Tree::Record(members), &mut kept);
        assert_eq!(kept, BTreeSet::from([3, 4, 5]));
    }
}
