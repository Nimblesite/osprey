import SwiftUI
import AppKit

// Implements [UI-COMPONENTS]: points and semantic surface tokens come from
// the shared Osprey tree. This renderer contains no workbench decisions.
public struct FrameStyle: Decodable, Sendable, Equatable {
    let name: String
    let width: Int
    let height: Int
    let top: Int
    let right: Int
    let bottom: Int
    let left: Int
    let radius: Int
    let gap: Int
    let fill: String
    let ink: String
    let line: String
    let lineEdge: String?
    let grow: Bool
    let font: String?
    let textSize: Int?
    let mathScale: Double?
    let lineHeight: Double?
    let tracking: Double?
    let italic: Bool?
    let hug: Bool?
    let align: String?
    let cross: String?
    let controlHeight: Int?
    let controlPadding: Int?
    let quietControls: Bool?
    let iconOnly: Bool?
    let fadeBackdrop: Bool?
    let trailingSymbols: Bool?
    let flatRows: Bool?
    let shadow: FrameShadow?
}

enum FrameShadow: String, Decodable, Sendable { case none = "", floating }

struct FramedView: View {
    let style: FrameStyle
    let content: Node
    let drawing: Drawing
    @Environment(\.ospreyControlPadding) private var inheritedControlPadding
    @Environment(\.ospreyPlacement) private var placement
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.ospreyInk) private var inheritedInk
    @Environment(\.ospreyTypography) private var typography

    private var alignment: Alignment { Alignment(horizontal: horizontal, vertical: vertical) }

    private var horizontal: HorizontalAlignment {
        switch style.cross {
        case "center": return .center
        case "end": return .trailing
        default: return .leading
        }
    }

    private var vertical: VerticalAlignment {
        switch style.align {
        case "center": return .center
        case "end": return .bottom
        default: return .top
        }
    }

    var body: some View {
        presentedContent
            .frame(width: style.width > 0 ? CGFloat(style.width) : nil, height: style.height > 0 ? CGFloat(style.height) : nil, alignment: alignment)
            .frame(maxWidth: style.width == 0 && style.hug != true ? .infinity : nil, maxHeight: style.grow ? .infinity : nil, alignment: alignment)
            .background { if !style.fill.isEmpty { theme.color(style.fill, dark: scheme == .dark) } }
            .clipShape(RoundedRectangle(cornerRadius: CGFloat(style.radius)))
            .overlay { border.allowsHitTesting(false) }
            .shadow(color: theme.tone(.primary, dark: scheme == .dark).opacity(style.shadow == .floating ? 2.0 / 15 : 0), radius: 25, x: 0, y: 20)
    }

    @ViewBuilder private var containedContent: some View {
        if style.grow && placement == .pane && !content.fillsPane {
            ViewThatFits(in: .vertical) {
                NodeView(node: content, drawing: drawing).environment(\.ospreyPlacement, .page)
                    .fixedSize(horizontal: false, vertical: true).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: alignment)
                PaneView(node: content, drawing: drawing)
            }
        } else {
            NodeView(node: content, drawing: drawing)
        }
    }

    private var presentedContent: some View {
        containedContent
            .environment(\.ospreyPlacement, style.grow ? placement : .page)
            .padding(EdgeInsets(top: CGFloat(style.top) + edgeInset("top"), leading: CGFloat(style.left) + edgeInset("left"), bottom: CGFloat(style.bottom) + edgeInset("bottom"), trailing: CGFloat(style.right) + edgeInset("right")))
            .environment(\.ospreyTypography, typography.applying(style))
            .modifier(FrameControls(style: style, padding: inheritedControlPadding))
            .environment(\.ospreyStackAlignment, style.align ?? "start")
            .environment(\.ospreyGap, CGFloat(style.gap))
            .environment(\.ospreyPresentation, style.name)
            .environment(\.ospreyInk, style.ink.isEmpty ? inheritedInk : style.ink)
    }

    private func edgeInset(_ name: String) -> CGFloat {
        let edge = style.lineEdge ?? "all"
        return style.line.isEmpty || (edge != "all" && edge != name) ? 0 : 1
    }

    @ViewBuilder private var border: some View {
        if !style.line.isEmpty {
            let color = theme.color(style.line, dark: scheme == .dark)
            switch style.lineEdge {
            case "left": color.frame(width: 1).frame(maxWidth: .infinity, alignment: .leading)
            case "right": color.frame(width: 1).frame(maxWidth: .infinity, alignment: .trailing)
            case "top": color.frame(height: 1).frame(maxHeight: .infinity, alignment: .top)
            case "bottom": color.frame(height: 1).frame(maxHeight: .infinity, alignment: .bottom)
            default: RoundedRectangle(cornerRadius: CGFloat(style.radius)).strokeBorder(color)
            }
        }
    }

}

private struct GapKey: EnvironmentKey {
    static let defaultValue: CGFloat? = nil
}

private struct PresentationKey: EnvironmentKey {
    static let defaultValue = ""
}

private struct InkKey: EnvironmentKey {
    static let defaultValue = ""
}

extension EnvironmentValues {
    var ospreyInk: String {
        get { self[InkKey.self] }
        set { self[InkKey.self] = newValue }
    }

    var ospreyPresentation: String {
        get { self[PresentationKey.self] }
        set { self[PresentationKey.self] = newValue }
    }

    var ospreyGap: CGFloat? {
        get { self[GapKey.self] }
        set { self[GapKey.self] = newValue }
    }
}

struct FrameTypography {
    var family = ""
    var size = 0
    var italic = false
    var lineHeight: Double = 0
    var tracking: Double = 0
    var mathScale: Double = 1

    func applying(_ style: FrameStyle) -> FrameTypography {
        FrameTypography(family: style.font?.isEmpty == false ? style.font ?? family : family,
            size: (style.textSize ?? 0) > 0 ? style.textSize ?? size : size,
            italic: style.font?.isEmpty == false || (style.textSize ?? 0) > 0 ? style.italic ?? false : italic,
            lineHeight: (style.lineHeight ?? 0) > 0 ? style.lineHeight ?? lineHeight : lineHeight,
            tracking: (style.tracking ?? 0) != 0 ? style.tracking ?? tracking : tracking,
            mathScale: (style.mathScale ?? 0) > 0 ? style.mathScale ?? mathScale : mathScale)
    }

    private var faceName: String {
        switch family {
        case "serif": return italic ? "STIXTwoText-Italic" : FontRegistration.serifName
        case "sans": return "Geist"
        case "mono": return "Menlo-Regular"
        default: return ".AppleSystemUIFont"
        }
    }

    var leadingGap: CGFloat {
        guard lineHeight > 0, size > 0 else { return 0 }
        let face = NSFont(name: faceName, size: CGFloat(size)) ?? NSFont.systemFont(ofSize: CGFloat(size))
        let lineBox = NSLayoutManager().defaultLineHeight(for: face) - face.leading.rounded()
        return CGFloat(lineHeight) - lineBox
    }

    // Prevent Float paragraph spacing from rounding upward into an extra pixel over long documents.
    var paragraphSpacing: CGFloat {
        let encoded = Float(leadingGap)
        return CGFloat(encoded) > leadingGap ? CGFloat(encoded.nextDown) : CGFloat(encoded)
    }

    func font(fallback: Font) -> Font {
        guard size > 0 else { return fallback }
        switch family {
        case "serif": return .custom(faceName, size: CGFloat(size))
        case "sans": return .custom("Geist", size: CGFloat(size))
        case "mono": return .custom(faceName, size: CGFloat(size))
        default: return .system(size: CGFloat(size))
        }
    }
}

private struct TypographyKey: EnvironmentKey {
    static let defaultValue = FrameTypography()
}

extension EnvironmentValues {
    var ospreyTypography: FrameTypography {
        get { self[TypographyKey.self] }
        set { self[TypographyKey.self] = newValue }
    }
}

private struct ControlHeightKey: EnvironmentKey {
    static let defaultValue = 0
}

private struct QuietControlsKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    var ospreyControlHeight: Int {
        get { self[ControlHeightKey.self] }
        set { self[ControlHeightKey.self] = newValue }
    }

    var ospreyQuietControls: Bool {
        get { self[QuietControlsKey.self] }
        set { self[QuietControlsKey.self] = newValue }
    }
}

struct DialogView: View {
    let title: String
    let content: Node
    let drawing: Drawing
    let dismiss: () -> Void

    var body: some View {
        ZStack(alignment: .topTrailing) {
            Color.clear.contentShape(Rectangle()).onTapGesture { }
            NodeView(node: content, drawing: drawing)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(title)
        .accessibilityAddTraits(.isModal)
        .onExitCommand(perform: dismiss)
    }
}

private struct IconOnlyKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    var ospreyIconOnly: Bool {
        get { self[IconOnlyKey.self] }
        set { self[IconOnlyKey.self] = newValue }
    }
}

public struct TextRun: Decodable, Sendable, Equatable {
    let text: String
    let ink: String
    let italic: Bool
}

struct RunsView: View {
    let runs: [TextRun]
    let role: TextRole
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.ospreyTypography) private var typography
    @Environment(\.ospreyInk) private var ink

    private func fragment(_ run: TextRun) -> Text {
        let color = run.ink.isEmpty ? (ink.isEmpty ? "primary" : ink) : run.ink
        let text = Text(run.text).foregroundColor(theme.color(color, dark: scheme == .dark))
        return run.italic ? text.italic() : text
    }

    var body: some View {
        runs.reduce(Text("")) { $0 + fragment($1) }
            .font(typography.font(fallback: ThemeResolver.font(role)))
            .tracking(typography.tracking)
            .lineSpacing(typography.paragraphSpacing)
            .padding(.vertical, typography.leadingGap / 2)
            .fixedSize(horizontal: false, vertical: true)
            .textSelection(.enabled)
    }
}

private struct FlatRowsKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    var ospreyFlatRows: Bool {
        get { self[FlatRowsKey.self] }
        set { self[FlatRowsKey.self] = newValue }
    }
}

private struct StackAlignmentKey: EnvironmentKey {
    static let defaultValue = "center"
}

extension EnvironmentValues {
    var ospreyStackAlignment: String {
        get { self[StackAlignmentKey.self] }
        set { self[StackAlignmentKey.self] = newValue }
    }
}

private struct ControlPaddingKey: EnvironmentKey {
    static let defaultValue: Int? = nil
}

extension EnvironmentValues {
    var ospreyControlPadding: Int? {
        get { self[ControlPaddingKey.self] }
        set { self[ControlPaddingKey.self] = newValue }
    }
}
