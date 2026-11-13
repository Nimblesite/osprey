import SwiftUI

// Implements [UI-COMPONENTS]: content nodes. Text is selectable, colours are
// theme tokens, and status always carries words as well as colour.

struct LeafView: View {
    let node: Tree
    @Environment(\.ospreyInherited) private var inherited
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    #if os(macOS)
    @Environment(\.ospreySelectionGroup) private var group
    @Environment(\.ospreyInk) private var frameInk
    #endif

    private var dark: Bool { scheme == .dark }

    var body: some View {
        switch node.text("kind") {
        case "text": plain
        case "rich-text": rich
        case "label": labelled
        case "icon": Image(systemName: Symbols.name(node.text("symbol"))).font(.system(size: inherited.symbolSize > 0 ? inherited.symbolSize : 14)).accessibilityHidden(true)
        case "badge": badge
        case "pair", "evidence": pair
        case "link": link
        case "spacer": Spacer(minLength: 0)
        case "divider": Divider()
        case "progress": progress
        case "meter": meter
        default: ContentUnavailableView(node.text("title"), systemImage: Symbols.name(node.text("symbol")), description: Text(node.text("detail"))).frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func tone(_ name: String) -> AnyShapeStyle {
        if name == "primary" { return theme.foreground(dark: dark) }
        if let colour = theme.color(name, dark: dark) { return AnyShapeStyle(colour) }
        return AnyShapeStyle(.secondary)
    }

    private func setting(_ text: Text, role: String) -> some View {
        text
            .font(Type.font(role, inherited))
            .tracking(tracking(role))
            .lineSpacing(leading(role))
            .fixedSize(horizontal: false, vertical: true)
    }

    private func tracking(_ role: String) -> CGFloat {
        inherited.tracking != 0 ? inherited.tracking : role == "eyebrow" ? 0.6 : 0
    }

    private func leading(_ role: String) -> CGFloat {
        let size = inherited.size > 0 ? inherited.size : 13
        return inherited.lineHeight > size ? inherited.lineHeight - size * 1.2 : 0
    }

    @ViewBuilder private var plain: some View {
        let role = node.text("role")
        let words = role == "eyebrow" ? node.text("text").uppercased() : node.text("text")
        #if os(macOS)
        if let group {
            setting(Text(words), role: role).hidden().overlay(alignment: .topLeading) {
                SelectableText(attributed: attributed(words, role: role, ink: nsTone(node.text("tone"))), group: group)
            }
        } else {
            setting(Text(words), role: role).foregroundStyle(tone(node.text("tone"))).textSelection(.enabled)
        }
        #else
        setting(Text(words), role: role).foregroundStyle(tone(node.text("tone"))).textSelection(.enabled)
        #endif
    }

    @ViewBuilder private var rich: some View {
        let role = node.text("role")
        let joined = node.list("runs").reduce(AttributedString()) { acc, run in acc + styled(run, role: role) }
        #if os(macOS)
        if let group {
            setting(Text(joined), role: role).hidden().overlay(alignment: .topLeading) {
                SelectableText(attributed: attributed(joined, role: role), group: group)
            }
        } else {
            setting(Text(joined), role: role).textSelection(.enabled)
        }
        #else
        setting(Text(joined), role: role).textSelection(.enabled)
        #endif
    }

    private func styled(_ run: Tree, role: String) -> AttributedString {
        var piece = AttributedString(run.text("text"))
        let base = Type.font(role, inherited)
        switch run.text("emphasis") {
        case "strong": piece.font = base.weight(.semibold)
        case "code":
            var mono = inherited
            mono.family = "mono"
            mono.size = (inherited.size > 0 ? inherited.size : 13) - 1
            piece.font = Type.font(role, mono)
            if let fill = theme.color("codeFill", dark: dark) { piece.backgroundColor = fill }
        default: if run.flag("italic") { piece.font = base.italic() }
        }
        if let ink = theme.color(run.text("ink"), dark: dark) { piece.foregroundColor = ink }
        #if os(macOS)
        // The same run for the AppKit text a selection group draws, which reads none of the above.
        var face = inherited
        if run.text("emphasis") == "code" {
            face.family = "mono"
            face.size = (inherited.size > 0 ? inherited.size : 13) - 1
            piece.appKit.backgroundColor = theme.color("codeFill", dark: dark).map(NSColor.init)
        } else if run.flag("italic") {
            face.italic = true
        }
        let font = Type.nsFont(role, face)
        piece.appKit.font = run.text("emphasis") == "strong" ? NSFont.systemFont(ofSize: font.pointSize, weight: .semibold) : font
        piece.appKit.foregroundColor = theme.color(run.text("ink"), dark: dark).map(NSColor.init)
        #endif
        return piece
    }

    #if os(macOS)
    /// The tone `tone` resolves, as AppKit ink for a leaf a selection group draws.
    private func nsTone(_ name: String) -> NSColor {
        if name == "primary" { return theme.color("text", dark: dark).map(NSColor.init) ?? .labelColor }
        return theme.color(name, dark: dark).map(NSColor.init) ?? .secondaryLabelColor
    }

    /// TextKit's line spacing, the same extra between lines the SwiftUI twin's `setting` adds.
    private func paragraph(_ role: String) -> NSParagraphStyle {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = leading(role)
        return paragraph
    }

    /// Plain words as AppKit draws them: this leaf's face, ink, tracking and spacing.
    private func attributed(_ words: String, role: String, ink: NSColor) -> NSAttributedString {
        var attributes: [NSAttributedString.Key: Any] = [.font: Type.nsFont(role, inherited), .foregroundColor: ink, .paragraphStyle: paragraph(role)]
        if tracking(role) != 0 { attributes[.kern] = tracking(role) }
        return NSAttributedString(string: words, attributes: attributes)
    }

    /// The runs `styled` built, over AppKit: faces and inks a run left unset fill from this leaf.
    private func attributed(_ joined: AttributedString, role: String) -> NSAttributedString {
        let appKit = (try? NSAttributedString(joined, including: \.appKit)) ?? NSAttributedString(string: String(joined.characters))
        let runs = NSMutableAttributedString(attributedString: appKit)
        let whole = NSRange(location: 0, length: runs.length)
        for (key, value) in [(NSAttributedString.Key.foregroundColor, frameInk ?? .labelColor), (.font, Type.nsFont(role, inherited))] {
            var missing: [NSRange] = []
            runs.enumerateAttribute(key, in: whole) { found, range, _ in if found == nil { missing.append(range) } }
            missing.forEach { runs.addAttribute(key, value: value, range: $0) }
        }
        runs.addAttributes([.paragraphStyle: paragraph(role)], range: whole)
        if tracking(role) != 0 { runs.addAttribute(.kern, value: tracking(role), range: whole) }
        return runs
    }
    #endif

    private var labelled: some View {
        HStack(spacing: 6) {
            if !node.text("symbol").isEmpty { Image(systemName: Symbols.name(node.text("symbol"))).font(.system(size: inherited.symbolSize > 0 ? inherited.symbolSize : 12)) }
            Text(node.text("label")).font(Type.font("body", inherited)).lineLimit(1)
        }
    }

    private var badge: some View {
        let colour = theme.color(node.text("tone"), dark: dark) ?? .secondary
        return HStack(spacing: 4) {
            if !node.text("symbol").isEmpty { Image(systemName: Symbols.name(node.text("symbol"))).font(.system(size: 9, weight: .bold)) }
            Text(node.text("label")).font(.system(size: 11, weight: .medium))
        }
        .foregroundStyle(colour)
        .padding(.horizontal, 8).padding(.vertical, 3)
        .background(colour.opacity(0.14), in: Capsule())
    }

    private var pair: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(node.text("label")).foregroundStyle(tone("secondary"))
            Spacer(minLength: 8)
            Text(node.text("value")).multilineTextAlignment(.trailing).textSelection(.enabled)
        }
        .font(Type.font("body", inherited))
    }

    @ViewBuilder private var link: some View {
        if let url = URL(string: node.text("url")), url.scheme == "https" {
            Link(node.text("label"), destination: url).font(Type.font("body", inherited))
        } else {
            Refusal(kind: "link to \(node.text("url"))")
        }
    }

    private var progress: some View {
        HStack(spacing: 8) {
            ProgressView().controlSize(.small)
            Shimmering(text: node.text("label")).font(Type.font("body", inherited))
            if !node.text("detail").isEmpty { Text(node.text("detail")).font(Type.font("callout", inherited)).foregroundStyle(tone("secondary")).lineLimit(1) }
        }
    }

    private var meter: some View {
        let share = Double(node.count("value")) / Double(max(node.count("total"), 1))
        let colour = theme.color(node.text("tone"), dark: dark) ?? .accentColor
        return VStack(alignment: .leading, spacing: 5) {
            if !node.text("label").isEmpty { Text(node.text("label")).font(Type.font("callout", inherited)).foregroundStyle(tone("secondary")) }
            GeometryReader { space in
                ZStack(alignment: .leading) {
                    Capsule().fill(colour.opacity(0.16))
                    Capsule().fill(colour).frame(width: max(4, space.size.width * share))
                }
            }
            .frame(height: 4)
        }
        .accessibilityElement(children: .combine)
    }
}

/// Words a light sweeps across while the agent works; Reduce Motion leaves them still.
struct Shimmering: View {
    let text: String
    @Environment(\.accessibilityReduceMotion) private var reduced
    @State private var moving = false

    var body: some View {
        Text(text)
            .foregroundStyle(.secondary)
            .overlay {
                if !reduced {
                    GeometryReader { space in
                        LinearGradient(colors: [.clear, .primary.opacity(0.9), .clear], startPoint: .leading, endPoint: .trailing)
                            .frame(width: space.size.width * 0.5)
                            .offset(x: moving ? space.size.width : -space.size.width * 0.5)
                    }
                    .mask(Text(text))
                }
            }
            .onAppear { withAnimation(.linear(duration: 1.6).repeatForever(autoreverses: false)) { moving = true } }
    }
}
