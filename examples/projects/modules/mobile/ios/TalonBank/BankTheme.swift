import SwiftUI

private struct BankViewportWidthKey: EnvironmentKey { static let defaultValue: CGFloat = 390 }
extension EnvironmentValues {
    var bankViewportWidth: CGFloat {
        get { self[BankViewportWidthKey.self] }
        set { self[BankViewportWidthKey.self] = newValue }
    }
}

enum BankTheme {
    static let ink = Color(hex: 0x0a2426)
    static let paper = Color(hex: 0xf7f7f2)
    static let coral = Color(hex: 0xff6b4a)
    static let mint = Color(hex: 0x9ee2c3)
    static let green = Color(hex: 0x1e795a)
    static let muted = Color(hex: 0x718987)
    static let line = Color(hex: 0xdde3df)
    static let softMint = Color(hex: 0xe5f8ef)
    static let red = Color(hex: 0xc83852)
    static let violet = Color(hex: 0x6556ae)

    static func type(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .custom("InterVariable", size: size).weight(weight)
    }

    static func font(_ node: BankNode, parent: BankNode?) -> Font {
        if parent?.has("hero-copy") == true, node.tag == "h1" { return type(43, weight: .bold) }
        if parent?.has("brand-copy") == true, node.tag == "strong" { return type(20, weight: .bold) }
        if node.has("account-balance") { return type(28, weight: .bold) }
        if parent?.has("detail-balance") == true, node.tag == "strong" { return type(36, weight: .bold) }
        if parent?.has("stat-copy") == true, node.tag == "strong" { return type(25, weight: .bold) }
        if node.has("eyebrow") || node.has("nav-section-label") { return type(10, weight: .heavy) }
        switch node.tag {
        case "h1": return type(34, weight: .bold)
        case "h2": return type(24, weight: .bold)
        case "h3": return type(17, weight: .semibold)
        case "small", "em", "dt": return type(12)
        case "strong", "dd": return type(15, weight: .semibold)
        default: return type(14)
        }
    }

    static func foreground(_ node: BankNode, dark: Bool) -> Color {
        if node.has("eyebrow") { return Color(hex: 0xd94d31) }
        if node.has("movement-amount") { return node.has("refused") ? red : node.has("credit") ? green : ink }
        if node.has("account-mark") && node.has("violet") { return violet }
        if node.has("movement-icon") || node.has("form-icon") {
            if node.has("refused") { return red }
            if node.has("debit") || node.has("withdraw") { return Color(hex: 0x96701e) }
            if node.has("transfer") { return violet }
            return green
        }
        if node.has("status-pill") || node.has("stat-icon") || node.has("security-icon") { return green }
        if ["small", "em", "p", "dt"].contains(node.tag) { return dark ? .white.opacity(0.68) : muted }
        return dark ? .white : ink
    }

    static func badgeBackground(_ node: BankNode) -> Color {
        guard node.isIcon || node.has("status-pill") else { return .clear }
        if node.has("toast-icon") { return .white.opacity(0.11) }
        if node.has("refused") { return Color(hex: 0xfff0f2) }
        if node.has("debit") || node.has("withdraw") { return Color(hex: 0xfff7df) }
        if node.has("violet") || node.has("transfer") { return Color(hex: 0xf0edff) }
        return softMint
    }
}

extension Color {
    init(hex: UInt32) {
        self.init(.sRGB, red: Double((hex >> 16) & 255) / 255,
                  green: Double((hex >> 8) & 255) / 255, blue: Double(hex & 255) / 255, opacity: 1)
    }
}

extension BankNode {
    var isDark: Bool { !classes.isDisjoint(with: ["hero-card", "guidance-card", "security-hero", "sidebar", "toast"]) }
    var isCard: Bool { !classes.isDisjoint(with: ["card", "stat-card", "mini-stat", "account-card"]) }
    var isRow: Bool {
        !classes.isDisjoint(with: ["brand", "account-card-top", "hero-actions", "detail-actions", "stat-card",
            "mini-stat", "movement-row", "security-control", "architecture-layer", "form-intro", "money-input",
            "modal-heading", "modal-reassurance", "shield-seal", "sidebar-profile", "secure-chip", "toast"])
    }
    var isIcon: Bool {
        !classes.isDisjoint(with: ["account-mark", "stat-icon", "movement-icon", "security-icon", "form-icon", "tip-icon", "avatar", "brand-mark", "toast-icon", "empty-icon"])
    }
    var spacing: CGFloat { has("page") ? 24 : has("field") ? 8 : 12 }
}

struct BankSurface: ViewModifier {
    let node: BankNode
    var padded: Bool { node.isCard || node.isDark || node.has("architecture-layer") || node.has("modal-form") || node.has("detail-balance") }
    var radius: CGFloat { node.isDark || node.has("modal") ? 26 : padded ? 20 : 0 }
    var background: Color {
        if node.has("toast") && node.has("error") { return Color(hex: 0x88273b) }
        if node.has("toast") && node.has("success") { return Color(hex: 0x155d48) }
        if node.isDark { return BankTheme.ink }
        if node.isCard || node.has("modal") { return .white }
        if node.has("architecture-layer") || node.has("detail-balance") { return Color(hex: 0xf2f4f0) }
        return .clear
    }
    func body(content: Content) -> some View {
        content.padding(padded ? 20 : 0)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(background)
            .clipShape(RoundedRectangle(cornerRadius: radius))
            .overlay(RoundedRectangle(cornerRadius: radius).stroke(node.isCard ? BankTheme.line.opacity(0.7) : .clear))
    }
}
