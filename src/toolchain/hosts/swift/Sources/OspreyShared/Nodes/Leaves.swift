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
        case "icon": NativeSymbol(name: node.text("symbol"), size: inherited.symbolSize > 0 ? inherited.symbolSize : FontRegistration.exactLines && inherited.size > 0 ? inherited.size : 14)
        case "badge": badge
        case "pair", "evidence": pair
        case "link": link
        case "spacer": Spacer(minLength: 0)
        case "divider": Divider()
        case "progress": progress
        case "meter": meter
        case "segments": segments
        case "swatch": SwatchLeaf(node: node)
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
            .padding(.vertical, exactGap(role) / 2)
            .fixedSize(horizontal: false, vertical: true)
    }

    /// The half-gap above and below each paragraph when an authored line height is exact.
    private func exactGap(_ role: String) -> CGFloat {
        #if os(macOS)
        return Type.leadingGap(role, inherited)
        #else
        return 0
        #endif
    }

    private func tracking(_ role: String) -> CGFloat {
        inherited.tracking != 0 ? inherited.tracking : role == "eyebrow" ? 0.6 : 0
    }

    private func leading(_ role: String) -> CGFloat {
        #if os(macOS)
        if FontRegistration.exactLines, inherited.lineHeight > 0 { return Type.lineGap(role, inherited) }
        #endif
        let size = inherited.size > 0 ? inherited.size : 13
        return inherited.lineHeight > size ? inherited.lineHeight - size * 1.2 : 0
    }

    @ViewBuilder private var plain: some View {
        let role = node.text("role")
        let words = role == "eyebrow" ? node.text("text").uppercased() : node.text("text")
        #if os(macOS)
        // An exact line box draws no line for no words, as the browser's empty paragraph.
        if FontRegistration.exactLines, words.isEmpty {
            Color.clear.frame(width: 0, height: 0)
        } else
        // Authored serif paragraphs wrap to exact line heights, as AppKit draws them.
        if group == nil, !words.isEmpty, role == "body", inherited.family == "serif", inherited.size > 0, inherited.lineHeight > 0 {
            ParagraphText(value: words, inherited: inherited, ink: nsTone(node.text("tone")))
        } else if let group {
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
            if !node.text("symbol").isEmpty { NativeSymbol(name: node.text("symbol"), size: inherited.symbolSize > 0 ? inherited.symbolSize : 12) }
            Text(node.text("label")).font(Type.font("body", inherited)).lineLimit(1)
        }
    }

    @ViewBuilder private var badge: some View {
        if FontRegistration.exactLines { outlinedBadge } else { filledBadge }
    }

    /// The bundled-face badge: words in the sans face on an outlined capsule the reference's
    /// 27 points tall, so a changing verdict keeps the document's rhythm ([WORKBENCH-DESIGN-TYPE]).
    private var outlinedBadge: some View {
        HStack(spacing: 6) {
            if !node.text("symbol").isEmpty { NativeSymbol(name: node.text("symbol"), size: 13) }
            Text(node.text("label"))
        }
        .font(.custom("Geist", size: 13))
        .foregroundStyle(theme.color(node.text("tone"), dark: dark) ?? .secondary)
        .padding(.horizontal, 10).padding(.vertical, 4)
        .frame(minHeight: 27)
        .overlay(Capsule().stroke(theme.color("outline", dark: dark) ?? .secondary, lineWidth: 1))
        .accessibilityElement(children: .combine)
    }

    private var filledBadge: some View {
        let colour = theme.color(node.text("tone"), dark: dark) ?? .secondary
        return HStack(spacing: 4) {
            if !node.text("symbol").isEmpty { NativeSymbol(name: node.text("symbol"), size: 9, weight: .bold) }
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

    /// A row of status segments, one per tone; the label says what they measure.
    private var segments: some View {
        HStack(spacing: 5) {
            ForEach(Array(node.list("tones").enumerated()), id: \.offset) { _, name in
                RoundedRectangle(cornerRadius: 3).fill(tone(name.string ?? "")).frame(height: 5)
            }
        }
        .accessibilityLabel(node.text("label"))
    }
}

/// One theme token's two appearances, side by side, named beneath.
struct SwatchLeaf: View {
    let node: Tree

    var body: some View {
        VStack(spacing: 4) {
            HStack(spacing: 0) {
                Rectangle().fill(ThemeResolver.parse(node.text("light")) ?? .clear)
                Rectangle().fill(ThemeResolver.parse(node.text("dark")) ?? .clear)
            }
            .frame(width: 56, height: 32).clipShape(RoundedRectangle(cornerRadius: 6))
            Text(node.text("name")).font(.caption2)
        }
        .accessibilityLabel("\(node.text("name")) \(node.text("light")) \(node.text("dark"))")
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

#if os(macOS)
/// An authored serif paragraph: word wrapping to exact line heights, without orphan-word
/// redistribution, as AppKit draws it.
struct ParagraphText: NSViewRepresentable {
    let value: String
    let inherited: Inherited
    let ink: NSColor

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField(wrappingLabelWithString: value)
        field.isSelectable = true
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineBreakMode = .byWordWrapping
        paragraph.lineBreakStrategy = []
        paragraph.minimumLineHeight = inherited.lineHeight
        paragraph.maximumLineHeight = inherited.lineHeight
        let font = Type.nsFont("body", inherited)
        let baseline = font.leading + (inherited.lineHeight - font.ascender + font.descender - font.leading) / 2
        let text = NSAttributedString(string: value, attributes: [.font: font, .foregroundColor: ink, .paragraphStyle: paragraph, .kern: inherited.tracking, .baselineOffset: baseline])
        if !field.attributedStringValue.isEqual(to: text) { field.attributedStringValue = text }
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSTextField, context: Context) -> CGSize? {
        guard let width = proposal.width, width > 0 else { return nil }
        nsView.preferredMaxLayoutWidth = width
        return CGSize(width: width, height: nsView.intrinsicContentSize.height)
    }
}
#endif
