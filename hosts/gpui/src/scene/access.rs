//! A scene's accessibility tree mapped to AccessKit ([UI-MACHINE-A11Y]): one invisible element per
//! described listener, at its bounds, with its role, label and, for a slider, its value and range.
//! An assistive action becomes the pointer press and release a mouse would send, in scene units, so
//! the application cannot tell a screen reader or an agent from a hand.

use gpui_kit::accesskit::{Action, ActionData};
use gpui_kit::*;
use serde_json::{json, Value};

use super::input;
use crate::events;
use crate::host::{now, HostView};
use crate::render::{self, Binding};

/// One element of the session's accessibility tree, bounds in scene units.
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
}

/// A slider steps a twentieth of its range per increment, as a scroll wheel notch would.
const STEPS: f64 = 20.;

/// The AccessKit role for a framework role name; `none` and unknown names are not announced.
fn role(name: &str) -> Option<Role> {
    match name {
        "button" | "popup" => Some(Role::Button),
        "slider" => Some(Role::Slider),
        "menuitem" => Some(Role::MenuItem),
        _ => None,
    }
}

/// Decodes the node's `access` list, keeping the elements a platform can announce.
pub(super) fn decode(node: &Value) -> Vec<Access> {
    render::list(node, "access")
        .iter()
        .filter_map(element)
        .collect()
}

fn element(item: &Value) -> Option<Access> {
    let number = |key: &str| item.get(key).and_then(Value::as_f64).unwrap_or(0.);
    Some(Access {
        role: role(item.get("role")?.as_str()?)?,
        label: item.get("label")?.as_str()?.to_owned(),
        x: number("x"),
        y: number("y"),
        w: number("w"),
        h: number("h"),
        value: number("value"),
        min: number("min"),
        max: number("max"),
    })
}

/// Where an assistive action lands, in scene units: a click at the element's centre, a set or
/// stepped value at its place along a slider; `None` for an action the element does not take.
pub fn target(access: &Access, action: Action, data: Option<&ActionData>) -> Option<(f64, f64)> {
    let step = (access.max - access.min) / STEPS;
    match (action, data) {
        (Action::Click, _) => Some((access.x + access.w / 2., access.y + access.h / 2.)),
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
            binding.region
        )))
        .absolute()
        .left(at(item.x))
        .top(at(item.y))
        .w(at(item.w))
        .h(at(item.h))
        .role(item.role)
        .aria_label(item.label.clone());
    let ranged = match item.role {
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
                acted(handle.clone(), binding.clone(), item.clone(), *action),
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
                let value = json!({"x": x, "y": y, "down": down, "shift": false});
                let event = events::control(
                    now(),
                    &binding.region,
                    input::current(view, &binding),
                    binding.ordinal,
                    value,
                );
                view.dispatch(event, window, cx);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{decode, target, Access, Action, ActionData, Role};
    use serde_json::Value;

    fn tree() -> Vec<Access> {
        let node: Value = serde_json::from_str(r#"{"access":[
            {"role":"button","label":"Play","x":10,"y":20,"w":100,"h":40,"value":0,"min":0,"max":0},
            {"role":"slider","label":"Volume","x":99,"y":1025,"w":302,"h":40,"value":0.5,"min":0,"max":1},
            {"role":"popup","label":"Speed: 1×","x":0,"y":0,"w":10,"h":10},
            {"role":"menuitem","label":"Kitchen","x":0,"y":50,"w":10,"h":10},
            {"role":"none","label":"Dismiss","x":0,"y":0,"w":400,"h":400},
            {"role":"button"}
        ]}"#).unwrap();
        decode(&node)
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
                (Role::MenuItem, "Kitchen")
            ]
        );
        assert_eq!(
            (tree[1].x, tree[1].w, tree[1].value, tree[1].max),
            (99., 302., 0.5, 1.)
        );
        assert_eq!((tree[2].min, tree[2].max), (0., 0.));
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
    }
}
