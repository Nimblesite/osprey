import SwiftUI

// Implements [UI-THEME]: semantic tokens from the first envelope resolved
// against the current appearance. The backend chooses no colour of its own;
// an unknown token is visibly missing rather than silently substituted.

public struct ThemeResolver: Sendable, Equatable {
    public let theme: Theme?

    public init(theme: Theme?) {
        self.theme = theme
    }

    public func color(_ name: String, dark: Bool) -> Color {
        guard let token = theme?.tokens.first(where: { $0.name == name }), let parsed = ThemeResolver.parse(dark ? token.dark : token.light) else { return .clear }
        return parsed
    }

    public func tone(_ tone: Tone, dark: Bool) -> Color { color(tone.rawValue, dark: dark) }

    /// A named token when the theme has it, such as the board's `board` and `chalk` ([UI-THEME-BOARD]).
    public func named(_ name: String, dark: Bool) -> Color? {
        guard let token = theme?.tokens.first(where: { $0.name == name }) else { return nil }
        return ThemeResolver.parse(dark ? token.dark : token.light)
    }

    func appearance(system: ColorScheme) -> ColorScheme {
        guard let canvas = named("canvas", dark: system == .dark) else { return system }
        let color = canvas.resolve(in: EnvironmentValues())
        let linear: (Float) -> Double = { channel in
            let value = Double(channel)
            return value <= 0.04045 ? value / 12.92 : pow((value + 0.055) / 1.055, 2.4)
        }
        let luminance = 0.2126 * linear(color.red) + 0.7152 * linear(color.green) + 0.0722 * linear(color.blue)
        let equalInkContrast = sqrt(0.05 * 1.05) - 0.05
        return luminance > equalInkContrast ? .light : .dark
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

    /// Motion durations in seconds; Reduce Motion removes them entirely.
    public func settle(reduced: Bool) -> Animation? {
        guard !reduced, let theme else { return nil }
        return .easeInOut(duration: Double(theme.settle) / 1000)
    }

    public func feedback(reduced: Bool) -> Animation? {
        guard !reduced, let theme else { return nil }
        return .easeOut(duration: Double(theme.feedback) / 1000)
    }

    public static func font(_ role: TextRole) -> Font {
        switch role {
        case .display: return .custom(FontRegistration.serifName, size: 44)
        case .large: return .largeTitle
        case .title: return .custom(FontRegistration.serifName, size: 29)
        case .heading: return .custom(FontRegistration.serifName, size: 28)
        case .lead: return .custom(FontRegistration.serifName, size: 24)
        case .headline: return .custom("Geist", size: 14).weight(.semibold)
        case .body: return .custom("Geist", size: 14)
        case .callout: return .custom("Geist", size: 14)
        case .caption: return .custom("Geist", size: 13)
        case .eyebrow: return .custom("Geist", size: 12).weight(.medium)
        case .mono: return .system(.callout, design: .monospaced)
        }
    }
}

private struct ThemeKey: EnvironmentKey {
    static let defaultValue = ThemeResolver(theme: nil)
}

extension EnvironmentValues {
    public var ospreyTheme: ThemeResolver {
        get { self[ThemeKey.self] }
        set { self[ThemeKey.self] = newValue }
    }
}
