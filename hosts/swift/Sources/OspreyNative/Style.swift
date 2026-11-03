import SwiftUI

// Implements [UI-THEME] and [UI-COMPONENTS-FRAMED]: the theme's tokens against
// the current appearance, and the typography and control metrics a frame passes
// down its subtree. Every value comes from the application; an unknown token
// draws nothing rather than a colour of this host's choosing.

public struct ThemeResolver: Sendable, Equatable {
    public let theme: Theme?

    public init(theme: Theme?) {
        self.theme = theme
    }

    public func color(_ name: String, dark: Bool) -> Color? {
        guard let token = theme?.tokens.first(where: { $0.name == name }) else { return nil }
        return ThemeResolver.parse(dark ? token.dark : token.light)
    }

    // [UI-THEME] Explicit foreground styles resolve the application's text token.
    func foreground(dark: Bool) -> AnyShapeStyle {
        if let colour = color("text", dark: dark) { return AnyShapeStyle(colour) }
        return AnyShapeStyle(.foreground)
    }

    /// `#RRGGBB` as a colour; anything else is unresolved.
    public static func parse(_ hex: String) -> Color? {
        guard hex.count == 7, hex.hasPrefix("#"), let value = UInt32(hex.dropFirst(), radix: 16) else { return nil }
        return Color(red: Double((value >> 16) & 0xFF) / 255, green: Double((value >> 8) & 0xFF) / 255, blue: Double(value & 0xFF) / 255)
    }

    public func spacing(_ step: Int) -> CGFloat {
        guard let values = theme?.spacing, step < values.count, step >= 0 else { return 8 }
        return CGFloat(values[step])
    }
}

/// What a frame's subtree inherits ([UI-COMPONENTS-FRAMED]).
public struct Inherited: Equatable {
    var gap: CGFloat?
    var family = ""
    var size: CGFloat = 0
    var lineHeight: CGFloat = 0
    var tracking: CGFloat = 0
    var italic = false
    var controlHeight: CGFloat = 0
    var controlPadding: CGFloat = -1
    var symbolSize: CGFloat = 0
    var quiet = false
    var iconOnly = false
    var trailingSymbols = false
    var flatRows = false
    var rowsFromTop = false

    func merging(_ style: [String: JSONValue]) -> Inherited {
        var next = self
        next.rowsFromTop = style["cross"]?.string == "start"
        if let value = style["gap"]?.int, value > 0 { next.gap = CGFloat(value) }
        if let value = style["font"]?.string, !value.isEmpty { next.family = value }
        if let value = style["textSize"]?.int, value > 0 { next.size = CGFloat(value) }
        if let value = style.double("lineHeight"), value > 0 { next.lineHeight = CGFloat(value) }
        if let value = style.double("tracking"), value != 0 { next.tracking = CGFloat(value) }
        if style["font"]?.string?.isEmpty == false || (style["textSize"]?.int ?? 0) > 0 { next.italic = style["italic"]?.bool ?? false }
        if let value = style["controlHeight"]?.int, value > 0 { next.controlHeight = CGFloat(value) }
        if let value = style["controlPadding"]?.int, value >= 0 { next.controlPadding = CGFloat(value) }
        if let value = style["symbolSize"]?.int, value > 0 { next.symbolSize = CGFloat(value) }
        if style["quietControls"]?.bool == true { next.quiet = true }
        if style["iconOnly"]?.bool == true { next.iconOnly = true }
        if style["trailingSymbols"]?.bool == true { next.trailingSymbols = true }
        if style["flatRows"]?.bool == true { next.flatRows = true }
        return next
    }
}

extension Dictionary where Key == String, Value == JSONValue {
    func double(_ key: String) -> Double? {
        if case let .number(value)? = self[key] { return value }
        return nil
    }
}

/// Where a node is drawn: the navigation sidebar's rows are the native source list.
public enum Placement: Sendable { case page, sidebar }

private struct ThemeKey: EnvironmentKey { static let defaultValue = ThemeResolver(theme: nil) }
private struct InheritedKey: EnvironmentKey { static let defaultValue = Inherited() }
private struct PlacementKey: EnvironmentKey { static let defaultValue = Placement.page }
private struct InRowKey: EnvironmentKey { static let defaultValue = false }
private struct RevealKey: EnvironmentKey { static let defaultValue: (@MainActor (UUID) -> Void)? = nil }

extension EnvironmentValues {
    public var ospreyTheme: ThemeResolver {
        get { self[ThemeKey.self] }
        set { self[ThemeKey.self] = newValue }
    }
    var ospreyReveal: (@MainActor (UUID) -> Void)? {
        get { self[RevealKey.self] }
        set { self[RevealKey.self] = newValue }
    }

    var ospreyInherited: Inherited {
        get { self[InheritedKey.self] }
        set { self[InheritedKey.self] = newValue }
    }
    var ospreyPlacement: Placement {
        get { self[PlacementKey.self] }
        set { self[PlacementKey.self] = newValue }
    }
    /// True for a direct child of a horizontal stack, where a frame keeps its natural width unless it grows.
    var ospreyInRow: Bool {
        get { self[InRowKey.self] }
        set { self[InRowKey.self] = newValue }
    }
}

enum Type {
    /// A role's face at the size and family a frame chose, else the platform's type scale.
    static func font(_ role: String, _ inherited: Inherited) -> Font {
        let design: Font.Design = inherited.family == "mono" || role == "mono" ? .monospaced : inherited.family == "serif" ? .serif : .default
        let size = inherited.size > 0 ? inherited.size : scale(role)
        let face = Font.system(size: size, weight: weight(role), design: design)
        return inherited.italic ? face.italic() : face
    }

    private static func scale(_ role: String) -> CGFloat {
        switch role {
        case "display": return 34
        case "large": return 28
        case "title": return 20
        case "heading": return 16
        case "lead": return 15
        case "headline": return 13
        case "callout": return 12
        case "caption": return 11
        case "eyebrow": return 11
        case "mono": return 12
        default: return 13
        }
    }

    private static func weight(_ role: String) -> Font.Weight {
        switch role {
        case "display", "large": return .semibold
        case "title", "heading", "headline", "eyebrow": return .semibold
        default: return .regular
        }
    }
}

/// [UI-COMPONENTS-SYMBOLS] Every framework symbol's SF Symbol; a name outside the table draws a visible mark.
public enum Symbols {
    public static let table: [String: String] = [
        "add": "plus", "delete": "trash", "dismiss": "xmark", "check": "checkmark", "warning": "exclamationmark.triangle",
        "info": "info.circle", "question": "questionmark.circle", "search": "magnifyingglass", "settings": "gearshape", "star": "star",
        "play": "play.fill", "pause": "pause.fill", "stop": "stop.fill", "refresh": "arrow.clockwise", "folder": "folder",
        "document": "doc.text", "package": "shippingbox", "inbox": "tray", "globe": "globe", "external": "arrow.up.forward.square",
        "lock": "lock", "shield": "checkmark.shield", "branch": "arrow.triangle.branch", "tag": "tag", "clock": "clock",
        "person": "person.crop.circle", "cursor": "cursorarrow", "chevron": "chevron.right",
        "send": "arrow.up", "terminal": "terminal", "edit": "square.and.pencil", "panel": "sidebar.right", "sparkle": "sparkle",
    ]

    static func name(_ symbol: String) -> String { table[symbol] ?? "questionmark.square.dashed" }
}
