use super::Ctx;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName};
use gpui_kit::*;

/// Maps a framework symbol name ([UI-COMPONENTS-SYMBOLS]) to its kit icon.
pub(crate) fn icon_name(name: &str) -> Option<IconName> {
    match name {
        "add" => Some(IconName::Plus),
        "delete" => Some(IconName::Delete),
        "dismiss" => Some(IconName::Close),
        "check" => Some(IconName::Check),
        "warning" => Some(IconName::TriangleAlert),
        "info" => Some(IconName::Info),
        "question" => Some(IconName::CircleAlert),
        "search" => Some(IconName::Search),
        "settings" => Some(IconName::Settings),
        "star" => Some(IconName::Star),
        "play" => Some(IconName::Play),
        "pause" => Some(IconName::Pause),
        "stop" => Some(IconName::Square),
        "refresh" => Some(IconName::RefreshCw),
        "folder" => Some(IconName::Folder),
        "document" => Some(IconName::File),
        "package" => Some(IconName::HardDrive),
        "inbox" => Some(IconName::Inbox),
        "globe" => Some(IconName::Globe),
        "external" => Some(IconName::ExternalLink),
        "lock" => Some(IconName::EyeOff),
        "shield" => Some(IconName::Ban),
        "branch" => Some(IconName::Network),
        "tag" => Some(IconName::Asterisk),
        "clock" => Some(IconName::Loader),
        "person" => Some(IconName::User),
        "cursor" => Some(IconName::Inspector),
        "chevron" => Some(IconName::ChevronRight),
        _ => None,
    }
}

/// Draws an icon or a visible empty square for an unknown symbol.
pub(crate) fn symbol(ctx: &mut Ctx, name: &str) -> AnyElement {
    match icon_name(name) {
        Some(icon) => Icon::new(icon).into_any_element(),
        None if name.is_empty() => div().into_any_element(),
        None => missing_symbol(ctx, name),
    }
}

/// Records an unknown symbol once and never silently omits its placeholder.
pub(crate) fn missing_symbol(ctx: &mut Ctx, name: &str) -> AnyElement {
    ctx.view.diagnose_once(format!("no icon for symbol {name}"));
    div()
        .size_4()
        .border_1()
        .border_color(ctx.cx.theme().foreground)
        .into_any_element()
}
