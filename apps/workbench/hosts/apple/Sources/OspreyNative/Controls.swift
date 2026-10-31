import SwiftUI

// Implements [UI-HOST] and [UI-EVENTS]: controls. Only native control state
// Osprey cannot observe stays here: the text being composed in a field.

/// Text wraps within the space offered and never forces its pane taller than the window; in a list row it is compact and leaves clicks to the row's selection.
struct TextView: View {
    let value: String
    let role: TextRole
    let tone: Tone
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.ospreyPlacement) private var placement
    @Environment(\.ospreyInk) private var ink
    @Environment(\.ospreyPresentation) private var presentation
    @Environment(\.ospreyTypography) private var typography

    var body: some View {
        let text = styledText.tracking(typography.tracking != 0 ? typography.tracking : role == .eyebrow ? 1.6 : 0).textCase(role == .eyebrow ? .uppercase : nil)
            .lineSpacing(typography.lineHeight > 0 ? typography.paragraphSpacing : role == .callout ? 4 : 0).padding(.vertical, typography.leadingGap / 2).foregroundStyle(resolvedColor)
        if !value.isEmpty && role == .body && typography.family == "serif" && typography.size > 0 && typography.lineHeight > 0 {
            ParagraphText(value: value, typography: typography, color: NSColor(resolvedColor))
        } else if !value.isEmpty {
            if placement == .row {
                text.lineLimit(role == .caption ? 2 : nil)
            } else {
                text.textSelection(.enabled)
            }
        } else { Color.clear.frame(width: 0, height: 0) }
    }

    private var styledText: Text {
        return Text(value).font(typography.font(fallback: ThemeResolver.font(role)))
    }

    private var resolvedColor: Color {
        ink.isEmpty ? theme.tone(tone, dark: scheme == .dark) : theme.color(ink, dark: scheme == .dark).opacity(tone == .secondary ? 0.78 : 1)
    }
}

/// A labelled observation whose value wraps and stays selectable; identifiers are never truncated.
struct EvidenceLine: View {
    let label: String
    let value: String
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(label).font(.caption).foregroundStyle(theme.tone(.secondary, dark: scheme == .dark))
            Text(value).font(.callout).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}

/// A web page opened in the person's browser; the symbol says it leaves the app.
struct LinkView: View {
    let label: String
    let url: URL
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.ospreyTypography) private var typography

    var body: some View {
        Link(destination: url) { Text(label).multilineTextAlignment(.leading) }
            .font(typography.font(fallback: .callout)).foregroundStyle(theme.tone(.accent, dark: scheme == .dark))
            .help(url.absoluteString)
    }
}

/// Status uses a quiet outlined capsule; its text carries the verdict ([DESIGN-STATUS]).
struct BadgeView: View {
    let label: String
    let symbol: String
    let color: Color
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        HStack(spacing: 6) {
            if !symbol.isEmpty { Image(systemName: symbol) }
            Text(label)
        }
        .font(.custom("Geist", size: 13))
        .foregroundStyle(color)
        .padding(.horizontal, 10).padding(.vertical, 4)
        .frame(minHeight: 27)
        .overlay(Capsule().stroke(theme.color("outline", dark: scheme == .dark), lineWidth: 1))
        .accessibilityElement(children: .combine)
    }
}

struct ProgressLine: View {
    let label: String
    let detail: String
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        HStack(spacing: theme.spacing(1)) {
            ProgressView().controlSize(.small)
            if !label.isEmpty { Text(label).font(.caption).foregroundStyle(theme.tone(.secondary, dark: scheme == .dark)) }
            if !detail.isEmpty { Text(detail).font(.caption) }
        }
    }
}

struct SegmentsView: View {
    let colors: [Color]
    let label: String

    var body: some View {
        HStack(spacing: 5) {
            ForEach(Array(colors.enumerated()), id: \.offset) { _, color in RoundedRectangle(cornerRadius: 3).fill(color).frame(height: 5) }
        }
        .accessibilityLabel(label)
    }
}

struct SwatchView: View {
    let name: String
    let light: String
    let dark: String

    var body: some View {
        VStack(spacing: 4) {
            HStack(spacing: 0) {
                Rectangle().fill(ThemeResolver.parse(light) ?? .clear)
                Rectangle().fill(ThemeResolver.parse(dark) ?? .clear)
            }
            .frame(width: 56, height: 32).clipShape(RoundedRectangle(cornerRadius: 6))
            Text(name).font(.caption2)
        }
        .accessibilityLabel("\(name) \(light) \(dark)")
    }
}

/// The application's mark, supplied by the application target; the backend names no product.
public struct BrandMark: Equatable {
    public let name: String
    public let image: Image?

    public init(name: String, image: Image?) {
        self.name = name
        self.image = image
    }

    public static func == (lhs: BrandMark, rhs: BrandMark) -> Bool { lhs.name == rhs.name }
}

private struct BrandKey: EnvironmentKey {
    static let defaultValue = BrandMark(name: "", image: nil)
}

extension EnvironmentValues {
    public var ospreyBrand: BrandMark {
        get { self[BrandKey.self] }
        set { self[BrandKey.self] = newValue }
    }
}

struct BrandView: View {
    @Environment(\.ospreyBrand) private var brand
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        HStack(spacing: 10) {
            brand.image?.resizable().interpolation(.high).frame(width: 36, height: 36).clipShape(RoundedRectangle(cornerRadius: 9)).accessibilityHidden(true)
            Text(brand.name).font(.system(size: 27, weight: .semibold, design: .rounded)).tracking(-1).foregroundStyle(theme.tone(.primary, dark: scheme == .dark))
        }
        .accessibilityElement(children: .combine)
    }
}

/// A toolbar shows the symbol; a page shows the themed button with its symbol and label.
struct ButtonView: View {
    let label: String
    let symbol: String
    let role: ButtonRole
    let enabled: Bool
    let tap: () -> Void
    @Environment(\.ospreyPlacement) private var placement
    @Environment(\.ospreyIconOnly) private var iconOnly
    @Environment(\.ospreyTrailingSymbols) private var trailingSymbols
    @FocusState private var focused: Bool

    var body: some View {
        control.focusable().focused($focused).focusEffectDisabled()
    }

    @ViewBuilder private var control: some View {
        if iconOnly {
            Button(action: tap) { NativeSymbol(name: symbol) }.accessibilityLabel(label).help(label).buttonStyle(ThemedButtonStyle(role: role, focused: focused)).disabled(!enabled)
        } else if placement == .toolbar && !trailingSymbols {
            Button(action: tap) { caption }.help(label).disabled(!enabled)
        } else {
            Button(role: role == .destructive ? .destructive : nil, action: tap) { caption }.buttonStyle(ThemedButtonStyle(role: role, focused: focused)).disabled(!enabled)
        }
    }

    @ViewBuilder private var caption: some View {
        if trailingSymbols && !symbol.isEmpty {
            HStack(spacing: 8) { Text(label); NativeSymbol(name: symbol) }
        } else if !symbol.isEmpty {
            HStack(spacing: 8) { NativeSymbol(name: symbol); Text(label) }
        } else { Text(label) }
    }
}

/// [DESIGN-MOTION]: feedback acknowledges a press; Reduce Motion removes it.
struct ThemedButtonStyle: ButtonStyle {
    let role: ButtonRole
    var focused = false
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.isEnabled) private var enabled
    @Environment(\.accessibilityReduceMotion) private var reduced
    @Environment(\.ospreyPresentation) private var presentation
    @Environment(\.ospreyInk) private var ink

    @Environment(\.ospreyQuietControls) private var compact
    @Environment(\.ospreyControlHeight) private var controlHeight
    @Environment(\.ospreyControlPadding) private var controlPadding
    @Environment(\.ospreyTypography) private var typography
    @Environment(\.ospreyIconOnly) private var iconOnly

    func makeBody(configuration: Configuration) -> some View {
        let dark = scheme == .dark
        let foreground: Tone = role == .destructive ? .danger : .primary
        return configuration.label.font(typography.font(fallback: .custom("Geist", size: 14))).lineLimit(1).fixedSize(horizontal: true, vertical: false)
            .padding(.horizontal, iconOnly ? 0 : CGFloat(controlPadding ?? (compact ? 12 : 16))).frame(maxWidth: iconOnly ? .infinity : nil).frame(minHeight: controlHeight > 0 ? CGFloat(controlHeight) : 44)
            .foregroundStyle(role == .filled ? theme.color("canvas", dark: dark) : ink.isEmpty ? theme.tone(role == .prominent ? .accent : foreground, dark: dark) : theme.color(ink, dark: dark))
            .background(role == .filled ? theme.color("accent", dark: dark) : role == .prominent ? theme.color("selection", dark: dark) : compact ? .clear : theme.color("inset", dark: dark), in: RoundedRectangle(cornerRadius: role == .filled ? 5 : compact ? 4 : 6, style: .continuous))
            .overlay { if focused { RoundedRectangle(cornerRadius: 8).stroke(theme.tone(.accent, dark: dark), lineWidth: 2).padding(-4).allowsHitTesting(false) } }
            .focusEffectDisabled()
            .opacity(enabled ? (configuration.isPressed ? 0.72 : 1) : 0.45)
            .animation(theme.feedback(reduced: reduced), value: configuration.isPressed)
    }
}

struct FieldView: View {
    let label: String
    let value: String
    let prompt: String
    let secure: Bool
    let multiline: Bool
    let commit: Commit
    let send: (String) -> Void
    @State private var draft: String
    @FocusState private var focused: Bool
    @Environment(\.ospreyPresentation) private var presentation

    init(label: String, value: String, prompt: String, secure: Bool, multiline: Bool, commit: Commit, send: @escaping (String) -> Void) {
        self.label = label
        self.value = value
        self.prompt = prompt
        self.secure = secure
        self.multiline = multiline
        self.commit = commit
        self.send = send
        _draft = State(initialValue: value)
    }

    var body: some View {
        // The label is a caption above a bordered field; the prompt is the field's placeholder.
        Group {
            if presentation == "solution-title" {
                input.focused($focused).textFieldStyle(.plain).font(.custom(FontRegistration.serifName, size: 29))
            } else {
                VStack(alignment: .leading, spacing: 4) {
                    if !label.isEmpty { Text(label).font(.caption) }
                    input.focused($focused).textFieldStyle(.roundedBorder).autocorrectionDisabled()
                }
            }
        }
        .onChange(of: draft) { _, next in if commit.edited(next, from: value) { send(next) } }
        .onChange(of: value) { _, next in if !focused { draft = next } }
        .onSubmit { if commit.submitted(draft, from: value) { send(draft) } }
    }

    @ViewBuilder private var input: some View {
        if secure {
            SecureField(label, text: $draft, prompt: Text(prompt))
        } else if multiline {
            TextField(label, text: $draft, prompt: Text(prompt), axis: .vertical).lineLimit(3...8)
        } else {
            TextField(label, text: $draft, prompt: Text(prompt))
        }
    }
}

struct PickerView: View {
    let label: String
    let choices: [Choice]
    let selected: String
    let choose: (String) -> Void
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme
    @Environment(\.controlSize) private var controlSize

    var body: some View {
        let insets = bezelInsets
        Picker(label, selection: Binding.sending(selected) { choose($0) }) {
            ForEach(choices, id: \.key) { choice in
                Text(choice.label).foregroundColor(theme.named("primary", dark: scheme == .dark) ?? .primary).tag(choice.key)
            }
        }
        // Implements [UI-COMPONENTS]: native bezel bounds must stay inside the authored pane.
        .padding(.leading, max(0, insets.left)).padding(.trailing, max(0, insets.right))
    }

    private var bezelInsets: NSEdgeInsets {
        let popup = NSPopUpButton(frame: .zero, pullsDown: false)
        switch controlSize {
        case .mini: popup.controlSize = .mini
        case .small: popup.controlSize = .small
        case .regular: popup.controlSize = .regular
        default: popup.controlSize = .large
        }
        return popup.alignmentRectInsets
    }
}

/// An alert node is invisible itself; it presents the platform alert while Osprey says it is visible.
/// A presented alert keeps its text until it closes: AppKit cannot rewrite one in place, so newer
/// text waits for the next presentation. Closing sends one dismissal.
struct AlertView: View {
    let title: String
    let message: String
    let visible: Bool
    let dismiss: () -> Void
    @State private var shown: Shown?

    struct Shown: Equatable {
        let title: String
        let message: String
    }

    var body: some View {
        Color.clear.frame(width: 0, height: 0)
            .alert(shown?.title ?? title, isPresented: Binding(get: { shown != nil }, set: { if !$0, shown != nil { shown = nil; dismiss() } }), presenting: shown) { _ in
                Button("OK", role: .cancel) {}
            } message: { Text($0.message) }
            .onAppear(perform: present)
            .onChange(of: visible) { present() }
            .onChange(of: Shown(title: title, message: message)) { present() }
    }

    // Osprey withdrawing the alert closes it without a dismissal of its own.
    private func present() {
        if !visible { shown = nil } else if shown == nil { shown = Shown(title: title, message: message) }
    }
}
