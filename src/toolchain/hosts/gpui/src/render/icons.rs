use gpui_kit::component::{ActiveTheme as _, Icon, IconName};
use gpui_kit::*;

/// What an icon needs from whichever renderer draws it: somewhere to record an unknown symbol
/// once, and the ink to draw its placeholder in.
pub(crate) trait Inked {
    fn ink(&mut self) -> Hsla;
    fn missing(&mut self, name: &str);
}

/// The component renderer's context.
impl Inked for crate::render::Ctx<'_, '_> {
    fn ink(&mut self) -> Hsla {
        self.cx.theme().foreground
    }

    fn missing(&mut self, name: &str) {
        self.view
            .diagnose_once(format!("no icon for symbol {name}"));
    }
}

/// The plain view-row renderer's context.
impl Inked for crate::view::Ctx<'_, '_> {
    fn ink(&mut self) -> Hsla {
        self.cx.theme().foreground
    }

    fn missing(&mut self, name: &str) {
        self.view
            .diagnose_once(format!("no icon for symbol {name}"));
    }
}

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
        "send" => Some(IconName::ArrowUp),
        "terminal" => Some(IconName::Inspector),
        "edit" => Some(IconName::Replace),
        "panel" => Some(IconName::PanelRight),
        "sparkle" => Some(IconName::Star),
        _ => None,
    }
}

/// Draws an icon or a visible empty square for an unknown symbol.
pub(crate) fn symbol(ctx: &mut impl Inked, name: &str) -> AnyElement {
    match icon_name(name) {
        Some(icon) => Icon::new(icon).into_any_element(),
        None if name.is_empty() => div().into_any_element(),
        None => missing_symbol(ctx, name),
    }
}

/// Records an unknown symbol once and never silently omits its placeholder.
pub(crate) fn missing_symbol(ctx: &mut impl Inked, name: &str) -> AnyElement {
    ctx.missing(name);
    div()
        .size_4()
        .border_1()
        .border_color(ctx.ink())
        .into_any_element()
}
