//! A scene's accessibility tree mapped to AccessKit ([UI-MACHINE-A11Y]): one invisible element per
//! described listener, at its bounds, with its role, label and, for a slider, its value and range.
//! An assistive action becomes the pointer press and release a mouse would send, in scene units, so
//! the application cannot tell a screen reader or an agent from a hand.

use crate::events::Touch;
use crate::host::HostView;
use crate::render::Binding;
use gpui_kit::accesskit::{Action, ActionData};
use gpui_kit::*;

/// One element of the session's accessibility tree, bounds in scene units.
// BEGIN GENERATED [UI-MACHINE-A11Y]
#[derive(Debug, Clone, PartialEq)]
pub struct Access {
    pub role: Role,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub selected: bool,
}
// END GENERATED [UI-MACHINE-A11Y]

/// A slider steps a twentieth of its range per increment, as a scroll wheel notch would.
const STEPS: f64 = 20.;

/// Where an assistive action lands, in scene units: a click at the element's centre, a set or
/// stepped value at its place along a slider; `None` for an action the element does not take.
pub fn target(access: &Access, action: Action, data: Option<&ActionData>) -> Option<(f64, f64)> {
    let step = (access.max - access.min) / STEPS;
    match (action, data) {
        (Action::Click, _) if access.role != Role::Label => {
            Some((access.x + access.w / 2., access.y + access.h / 2.))
        }
        (Action::SetValue, Some(ActionData::NumericValue(value))) => along(access, *value),
        (Action::Increment, _) => along(access, access.value + step),
        (Action::Decrement, _) => along(access, access.value - step),
        _ => None,
    }
}

/// The point at `value` along a slider, clamped to its range; `None` when it has no range.
fn along(access: &Access, value: f64) -> Option<(f64, f64)> {
    let span = access.max - access.min;
    (access.role == Role::Slider && span > 0.).then(|| {
        let fraction = ((value - access.min) / span).clamp(0., 1.);
        (access.x + access.w * fraction, access.y + access.h / 2.)
    })
}

/// The actions an element takes: every element clicks; a slider also sets and steps its value.
fn actions(access: &Access) -> &'static [Action] {
    match access.role {
        Role::Label => &[],
        Role::Slider => &[
            Action::Click,
            Action::SetValue,
            Action::Increment,
            Action::Decrement,
        ],
        _ => &[Action::Click],
    }
}

/// The invisible accessible elements laid over a scene drawn at `scale` window pixels per unit.
pub(super) fn overlay(
    handle: &Entity<HostView>,
    binding: &Binding,
    access: &[Access],
    scale: f32,
) -> Vec<AnyElement> {
    access
        .iter()
        .enumerate()
        .map(|(index, item)| node(handle, binding, item, index, scale))
        .collect()
}

fn node(
    handle: &Entity<HostView>,
    binding: &Binding,
    item: &Access,
    index: usize,
    scale: f32,
) -> AnyElement {
    let at = |value: f64| px(value as f32 * scale);
    let base = div()
        .id(SharedString::from(format!(
            "{}:a11y:{index}",
            binding.ordinal
        )))
        .absolute()
        .left(at(item.x))
        .top(at(item.y))
        .w(at(item.w))
        .h(at(item.h))
        .role(item.role)
        .aria_label(item.label.clone());
    let ranged = match item.role {
        Role::MenuItem => base.aria_selected(item.selected),
        Role::Slider => base
            .aria_numeric_value(item.value)
            .aria_min_numeric_value(item.min)
            .aria_max_numeric_value(item.max),
        _ => base,
    };
    actions(item)
        .iter()
        .fold(ranged, |element, action| {
            element.on_a11y_action(
                *action,
                acted(handle.clone(), *binding, item.clone(), *action),
            )
        })
        .into_any_element()
}

/// Sends the press and release an assistive action lands as.
fn acted(
    handle: Entity<HostView>,
    binding: Binding,
    item: Access,
    action: Action,
) -> impl FnMut(Option<&ActionData>, &mut Window, &mut App) + 'static {
    move |data, window, cx| {
        let Some((x, y)) = target(&item, action, data) else {
            return;
        };
        handle.update(cx, |view, cx| {
            for down in [true, false] {
                view.send(
                    Touch::pointer(binding.ordinal, x, y, down, false),
                    window,
                    cx,
                );
            }
        });
    }
}

/// Announced scene elements as the tests build them.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::{Access, Role};

    /// One element at `bounds` (`x, y, w, h`) with its `range` (`value, min, max`).
    pub(crate) fn element(
        role: Role,
        label: &str,
        bounds: [f64; 4],
        range: [f64; 3],
        selected: bool,
    ) -> Access {
        let [x, y, w, h] = bounds;
        let [value, min, max] = range;
        Access {
            role,
            label: label.to_owned(),
            x,
            y,
            w,
            h,
            value,
            min,
            max,
            selected,
        }
    }

    /// A button at `bounds`.
    pub(crate) fn button(label: &str, bounds: [f64; 4]) -> Access {
        element(Role::Button, label, bounds, [0.; 3], false)
    }

    /// A slider at `bounds` holding `range`.
    pub(crate) fn slider(label: &str, bounds: [f64; 4], range: [f64; 3]) -> Access {
        element(Role::Slider, label, bounds, range, false)
    }

    /// A menu item at `bounds`, the current choice when `selected`.
    pub(crate) fn menu_item(label: &str, bounds: [f64; 4], selected: bool) -> Access {
        element(Role::MenuItem, label, bounds, [0.; 3], selected)
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{button, menu_item, slider};
    use super::{actions, target, Access, Action, ActionData, Role};

    fn tree() -> Vec<Access> {
        vec![
            button("Play", [10., 20., 100., 40.]),
            slider("Volume", [99., 1025., 302., 40.], [0.5, 0., 1.]),
            button("Speed: 1×", [0., 0., 10., 10.]),
            menu_item("Kitchen", [0., 50., 10., 10.], true),
            menu_item("Hallway", [0., 60., 10., 10.], false),
        ]
    }

    /// Every announced role maps to AccessKit; `none` and an element without a label are dropped.
    #[test]
    fn decodes_roles_labels_bounds_and_ranges() {
        let tree = tree();
        let roles: Vec<_> = tree.iter().map(|a| (a.role, a.label.as_str())).collect();
        assert_eq!(
            roles,
            vec![
                (Role::Button, "Play"),
                (Role::Slider, "Volume"),
                (Role::Button, "Speed: 1×"),
                (Role::MenuItem, "Kitchen"),
                (Role::MenuItem, "Hallway")
            ]
        );
        assert_eq!(
            (tree[1].x, tree[1].w, tree[1].value, tree[1].max),
            (99., 302., 0.5, 1.)
        );
        assert_eq!((tree[2].min, tree[2].max), (0., 0.));
    }

    #[test]
    fn menu_items_carry_current_selection_independently_of_visuals() {
        let items = tree();
        assert!(items[3].selected);
        assert!(!items[4].selected);
        assert!(items[..3].iter().all(|item| !item.selected));
    }

    /// A click lands at the centre; a slider's set, increment and decrement land along it, clamped.
    #[test]
    fn actions_land_where_a_pointer_would() {
        let tree = tree();
        let (play, volume) = (&tree[0], &tree[1]);
        assert_eq!(target(play, Action::Click, None), Some((60., 40.)));
        assert_eq!(
            target(play, Action::SetValue, Some(&ActionData::NumericValue(0.5))),
            None
        );
        assert_eq!(target(play, Action::Increment, None), None);
        assert_eq!(
            target(
                volume,
                Action::SetValue,
                Some(&ActionData::NumericValue(0.25))
            ),
            Some((99. + 302. * 0.25, 1045.))
        );
        assert_eq!(
            target(
                volume,
                Action::SetValue,
                Some(&ActionData::NumericValue(9.))
            ),
            Some((401., 1045.))
        );
        assert_eq!(
            target(
                volume,
                Action::SetValue,
                Some(&ActionData::NumericValue(-1.))
            ),
            Some((99., 1045.))
        );
        assert_eq!(target(volume, Action::SetValue, None), None);
        assert_eq!(
            target(volume, Action::Increment, None),
            Some((99. + 302. * 0.55, 1045.))
        );
        assert_eq!(
            target(volume, Action::Decrement, None),
            Some((99. + 302. * 0.45, 1045.))
        );
        assert_eq!(target(volume, Action::Click, None), Some((250., 1045.)));
        assert_eq!(target(volume, Action::Focus, None), None);
        // [GPUI-HOST-SCENE-TEXT-A11Y] Reading a title cannot activate a player.
        let title = Access {
            role: Role::Label,
            ..play.clone()
        };
        assert_eq!(target(&title, Action::Click, None), None);
        assert_eq!(target(&title, Action::Increment, None), None);
        assert!(actions(&title).is_empty());
    }
}
