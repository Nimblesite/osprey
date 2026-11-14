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
    var mathScale: Double = 1
    var italic = false
    var hug = false
    var controlHeight: CGFloat = 0
    var controlPadding: CGFloat = -1
    var symbolSize: CGFloat = 0
    var quiet = false
    var iconOnly = false
    var trailingSymbols = false
    var flatRows = false
    var fadeBackdrop = false
    var rowsFromTop = false
    /// How a stack places its children on the cross axis, as the frame authored it
    /// ([UI-COMPONENTS-FRAMED]): start, center or end.
    var cross = "start"

    func merging(_ style: [String: Tree]) -> Inherited {
        var next = self
        next.rowsFromTop = style["cross"]?.string == "start"
        if let value = style["cross"]?.string, !value.isEmpty { next.cross = value }
        if let value = style["gap"]?.int, value > 0 || (FontRegistration.exactLines && value == 0) { next.gap = CGFloat(value) }
        if let value = style["font"]?.string, !value.isEmpty { next.family = value }
        if let value = style["textSize"]?.int, value > 0 { next.size = CGFloat(value) }
        if let value = style.double("lineHeight"), value > 0 { next.lineHeight = CGFloat(value) }
        if let value = style.double("tracking"), value != 0 { next.tracking = CGFloat(value) }
        if let value = style.double("mathScale"), value > 0 { next.mathScale = value }
        if style["hug"]?.bool == true { next.hug = true }
        if style["font"]?.string?.isEmpty == false || (style["textSize"]?.int ?? 0) > 0 { next.italic = style["italic"]?.bool ?? false }
        if let value = style["controlHeight"]?.int, value > 0 { next.controlHeight = CGFloat(value) }
        if let value = style["controlPadding"]?.int, value >= 0 { next.controlPadding = CGFloat(value) }
        if let value = style["symbolSize"]?.int, value > 0 { next.symbolSize = CGFloat(value) }
        if style["quietControls"]?.bool == true { next.quiet = true }
        if style["iconOnly"]?.bool == true { next.iconOnly = true }
        if style["trailingSymbols"]?.bool == true { next.trailingSymbols = true }
        if style["flatRows"]?.bool == true { next.flatRows = true }
        if style["fadeBackdrop"]?.bool == true { next.fadeBackdrop = true }
        return next
    }
}

extension Dictionary where Key == String, Value == Tree {
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
    /// A role's size, weight, family and slant; the one table both font systems read.
    private static func face(_ role: String, _ inherited: Inherited) -> (size: CGFloat, bold: Bool, serif: Bool, mono: Bool) {
        (inherited.size > 0 ? inherited.size : scale(role), bold(role), inherited.family == "serif", inherited.family == "mono" || role == "mono")
    }

    /// The face name an authored family registers: the serif and sans faces applications bundle.
    /// Mono stays the platform's monospaced face, so every host's code looks native.
    private static func authored(_ inherited: Inherited) -> String? {
        switch inherited.family {
        case "serif": return inherited.italic ? FontRegistration.serifItalicName : FontRegistration.serifName
        case "sans": return "Geist"
        default: return nil
        }
    }

    /// A role's face at the size and family a frame chose, else the platform's type scale. An
    /// authored size draws the bundled face at exactly that size, regular as authored.
    static func font(_ role: String, _ inherited: Inherited) -> Font {
        if inherited.size > 0, let name = authored(inherited) { return .custom(name, size: inherited.size) }
        let face = self.face(role, inherited)
        let base = Font.system(size: face.size, weight: face.bold ? .semibold : .regular, design: face.mono ? .monospaced : face.serif ? .serif : .default)
        return inherited.italic ? base.italic() : base
    }

    #if os(macOS)
    /// The same face as an AppKit font, for a leaf a selection group draws.
    static func nsFont(_ role: String, _ inherited: Inherited) -> NSFont {
        if inherited.size > 0, let name = authored(inherited), let font = NSFont(name: name, size: inherited.size) { return font }
        let face = self.face(role, inherited)
        let weight: NSFont.Weight = face.bold ? .semibold : .regular
        let base: NSFont
        if face.mono {
            base = NSFont.monospacedSystemFont(ofSize: face.size, weight: weight)
        } else if face.serif, let design = NSFont.systemFont(ofSize: face.size, weight: weight).fontDescriptor.withDesign(.serif) {
            base = NSFont(descriptor: design, size: face.size) ?? NSFont.systemFont(ofSize: face.size, weight: weight)
        } else {
            base = NSFont.systemFont(ofSize: face.size, weight: weight)
        }
        return inherited.italic ? NSFontManager.shared.convert(base, toHaveTrait: .italicFontMask) : base
    }
    #endif

    #if os(macOS)
    /// What an authored line height adds to one line of `role`'s face: the browser's line box
    /// less the face's own, so every line takes exactly the authored height.
    static func leadingGap(_ role: String, _ inherited: Inherited) -> CGFloat {
        guard FontRegistration.exactLines, inherited.lineHeight > 0 else { return 0 }
        let face = nsFont(role, inherited)
        return inherited.lineHeight - (NSLayoutManager().defaultLineHeight(for: face) - face.leading.rounded())
    }

    /// The same gap between lines, never rounded upward, so long documents gain no pixel.
    static func lineGap(_ role: String, _ inherited: Inherited) -> CGFloat {
        let gap = leadingGap(role, inherited)
        let encoded = Float(gap)
        return CGFloat(encoded) > gap ? CGFloat(encoded.nextDown) : CGFloat(encoded)
    }
    #endif

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

    private static func bold(_ role: String) -> Bool {
        switch role {
        case "display", "large", "title", "heading", "headline", "eyebrow": return true
        default: return false
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
