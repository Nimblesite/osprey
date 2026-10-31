import SwiftUI

private struct ViewportSizeKey: EnvironmentKey { static let defaultValue = CGSize.zero }

extension EnvironmentValues {
    var ospreyViewportSize: CGSize {
        get { self[ViewportSizeKey.self] }
        set { self[ViewportSizeKey.self] = newValue }
    }
}

struct ViewportContent<Content: View>: View {
    let active: Bool
    let content: Content

    init(active: Bool = true, @ViewBuilder content: () -> Content) { self.active = active; self.content = content() }

    var body: some View {
        if active {
            GeometryReader { geometry in
                ScrollView(.vertical) {
                    content.frame(minWidth: geometry.size.width, minHeight: geometry.size.height, alignment: .topLeading)
                }
                .environment(\.ospreyViewportSize, geometry.size)
            }
        } else { content }
    }
}

extension Node {
    var authorsViewport: Bool {
        switch self {
        case let .framed(style, child): return (style.viewportHeightInset ?? -1) >= 0 || child.authorsViewport
        case let .stack(_, nodes), let .layer(nodes), let .split(nodes), let .section(_, _, nodes), let .disclosure(_, _, _, nodes): return nodes.contains(where: \.authorsViewport)
        case let .themed(_, child), let .dialog(_, child, _): return child.authorsViewport
        case let .screen(_, _, toolbar, nodes): return (toolbar + nodes).contains(where: \.authorsViewport)
        case let .aside(pane, _, aside, _): return pane.authorsViewport || aside.authorsViewport
        case let .tabs(entries, _, _): return entries.contains { $0.child.authorsViewport }
        case let .rows(entries, _, _): return entries.contains { $0.child.authorsViewport }
        case let .contextMenu(body, items): return body.authorsViewport || items.contains(where: \.authorsViewport)
        default: return false
        }
    }
}
