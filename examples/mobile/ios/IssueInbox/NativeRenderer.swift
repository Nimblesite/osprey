import SwiftUI

// Implements [MOBILE-REACTIVE-UI]. All labels, layout and events come from Osprey.
struct InboxNode: Decodable {
    let kind: String
    var id: String?
    var text: String?
    var style: String?
    var value: String?
    var placeholder: String?
    var url: String?
    var submit: Bool?
    var event: [String: String]?
    var children: [InboxNode]?
    var spans: [InboxSpan]?
}

// One run of a `rich` node: Osprey chose the style and link. [MOBILE-MARKDOWN]
struct InboxSpan: Decodable {
    let text: String
    let style: String
    var url: String?
}

struct NativeRenderer: View {
    let node: InboxNode
    let send: ([String: Any]) -> Void

    var body: some View {
        rendered.modifier(NodeStyle(style: node.style ?? ""))
            .accessibilityIdentifier(node.id ?? "")
    }

    private var children: some View {
        ForEach(Array((node.children ?? []).enumerated()), id: \.offset) { _, child in
            NativeRenderer(node: child, send: send)
        }
    }

    private var rendered: AnyView {
        switch node.kind {
        case "column": return AnyView(VStack(alignment: .leading, spacing: 14) { children })
        case "row": return AnyView(HStack(alignment: node.style == "item" ? .top : .center,
                                          spacing: node.style == "item" ? 8 : 14) { children })
        case "scroll": return AnyView(ScrollView { children })
        case "text": return AnyView(Text(node.text ?? "").fixedSize(horizontal: false, vertical: true))
        case "rich": return AnyView(Text(richText).fixedSize(horizontal: false, vertical: true))
        case "button": return AnyView(Button(node.text ?? "") { send(node.event ?? [:]) })
        case "input": return AnyView(NativeInput(node: node, send: send))
        case "link": return link
        case "divider": return AnyView(Divider())
        default: return AnyView(Text("Unsupported native view: \(node.kind)").foregroundColor(.red))
        }
    }

    // Implements [MOBILE-MARKDOWN]: spans carry Osprey's emphasis, code and
    // HTTPS link decisions; SwiftUI only applies the platform presentation.
    private var richText: AttributedString {
        (node.spans ?? []).reduce(into: AttributedString()) { result, span in
            var run = AttributedString(span.text)
            switch span.style {
            case "bold": run.inlinePresentationIntent = .stronglyEmphasized
            case "italic": run.inlinePresentationIntent = .emphasized
            case "code": run.inlinePresentationIntent = .code
            default: break
            }
            if let raw = span.url, let url = URL(string: raw), url.scheme == "https" { run.link = url }
            result.append(run)
        }
    }

    private var link: AnyView {
        guard let raw = node.url, let url = URL(string: raw), url.scheme == "https" else {
            return AnyView(Text("Invalid link").foregroundColor(.red))
        }
        return AnyView(Link(node.text ?? raw, destination: url))
    }
}

private struct NativeInput: View {
    let node: InboxNode
    let send: ([String: Any]) -> Void
    @State private var value: String
    @FocusState private var focused: Bool

    init(node: InboxNode, send: @escaping ([String: Any]) -> Void) {
        self.node = node
        self.send = send
        _value = State(initialValue: node.value ?? "")
    }

    var body: some View {
        TextField(node.placeholder ?? "", text: $value)
            .textInputAutocapitalization(.never).autocorrectionDisabled()
            .submitLabel(node.submit == true ? .go : .search).focused($focused)
            .padding(13).background(Color(.tertiarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 12))
            .onChange(of: value) { _ in if node.submit != true { dispatch() } }
            .onChange(of: node.value) { next in if !focused { value = next ?? "" } }
            .onSubmit { dispatch(); focused = false }
    }

    private func dispatch() {
        var event: [String: Any] = node.event ?? [:]
        event["value"] = value
        send(event)
    }
}

private struct NodeStyle: ViewModifier {
    let style: String
    @Environment(\.colorScheme) private var scheme
    private let fill = Color(red: 0.29, green: 0.25, blue: 0.81)
    private var accent: Color {
        scheme == .dark ? Color(red: 0.68, green: 0.65, blue: 1) : fill
    }

    @ViewBuilder func body(content: Content) -> some View {
        switch style {
        case "screen": content.background(Color(.systemGroupedBackground)).tint(accent)
        case "page": content.padding(20).frame(maxWidth: 720)
        case "header": content.padding(.vertical, 14).frame(maxWidth: .infinity, alignment: .leading)
        case "card": content.padding(18).frame(maxWidth: .infinity, alignment: .leading)
                .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 20))
        case "hero": content.font(.system(size: 36, weight: .bold, design: .rounded))
        case "title": content.font(.headline)
        case "caption": content.font(.caption).foregroundColor(.secondary)
        case "accent": content.font(.caption.weight(.bold)).foregroundColor(accent)
        case "error": content.font(.callout).foregroundColor(.red)
        case "heading": content.font(.title3.weight(.semibold))
        case "subheading": content.font(.headline)
        case "code": content.font(.system(.footnote, design: .monospaced)).padding(12).frame(maxWidth: .infinity, alignment: .leading)
                .background(Color(.tertiarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 10))
        case "quote": content.padding(.leading, 12).foregroundColor(.secondary)
                .overlay(alignment: .leading) { RoundedRectangle(cornerRadius: 2).fill(accent.opacity(0.6)).frame(width: 3) }
        case "bullet": content.foregroundColor(.secondary)
        case "item": content.frame(maxWidth: .infinity, alignment: .leading)
        case "primary": content.font(.subheadline.weight(.semibold)).padding(.horizontal, 16).padding(.vertical, 10)
                .foregroundColor(.white).background(fill, in: Capsule())
        case "secondary": content.font(.subheadline.weight(.medium)).padding(.horizontal, 12).padding(.vertical, 10)
                .foregroundColor(accent).background(accent.opacity(0.09), in: Capsule())
        default: content
        }
    }
}
