import SwiftUI

// Implements [UI-COMPONENTS]: content nodes. Text is selectable, colours are
// theme tokens, and status always carries words as well as colour.

struct LeafView: View {
    let node: JSONValue
    @Environment(\.ospreyInherited) private var inherited
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

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
        if name == "primary" { return AnyShapeStyle(.foreground) }
        if let colour = theme.color(name, dark: dark) { return AnyShapeStyle(colour) }
        return AnyShapeStyle(.secondary)
    }

    private func setting(_ text: Text, role: String) -> some View {
        text
            .font(Type.font(role, inherited))
            .tracking(inherited.tracking != 0 ? inherited.tracking : role == "eyebrow" ? 0.6 : 0)
            .lineSpacing(leading(role))
            .fixedSize(horizontal: false, vertical: true)
    }

    private func leading(_ role: String) -> CGFloat {
        let size = inherited.size > 0 ? inherited.size : 13
        return inherited.lineHeight > size ? inherited.lineHeight - size * 1.2 : 0
    }

    private var plain: some View {
        let role = node.text("role")
        let words = role == "eyebrow" ? node.text("text").uppercased() : node.text("text")
        return setting(Text(words), role: role).foregroundStyle(tone(node.text("tone"))).textSelection(.enabled)
    }

    private var rich: some View {
        let role = node.text("role")
        let joined = node.list("runs").reduce(AttributedString()) { acc, run in acc + styled(run, role: role) }
        return setting(Text(joined), role: role).textSelection(.enabled)
    }

    private func styled(_ run: JSONValue, role: String) -> AttributedString {
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
        return piece
    }

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
