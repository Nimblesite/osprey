import SwiftUI

// Implements [UI-HOST] and [UI-DIAGNOSTICS]: the window shows the root region
// and the framework's diagnostics, and logs content that outgrows the window;
// nothing else is composed here.

public struct RootView: View {
    @ObservedObject var store: RegionStore
    @Environment(\.colorScheme) private var scheme

    public init(store: RegionStore) {
        self.store = store
    }

    public var body: some View {
        ViewportContent(active: store.usesViewportLayout) {
            regionContent
                .foregroundStyle(ThemeResolver(theme: store.theme).tone(.primary, dark: scheme == .dark))
                .background(ThemeResolver(theme: store.theme).color("canvas", dark: scheme == .dark))
                .modifier(OverflowLog())
                .tint(ThemeResolver(theme: store.theme).tone(.accent, dark: scheme == .dark))
                .environmentObject(store)
                .environment(\.ospreyTheme, ThemeResolver(theme: store.theme))
                .task { store.start() }
        }
    }

    private var regionContent: some View {
        VStack(spacing: 0) {
            if let failure = store.failure {
                ContentUnavailableView("The application could not start", systemImage: "exclamationmark.triangle", description: Text(failure))
            } else if let root = store.root {
                CellView(cell: root, store: store).environment(\.ospreyPlacement, store.usesViewportLayout ? .page : .pane)
            } else {
                ProgressView("Starting")
            }
            if let last = store.diagnostics.last, store.failure == nil {
                Text(last).font(.caption).foregroundStyle(.secondary).padding(6).frame(maxWidth: .infinity, alignment: .leading).background(.bar)
            }
        }
    }
}

/// Window content larger than its window is centred off-screen, so the window looks empty. That is a
/// layout defect, logged as a fault with the content's size and position.
struct OverflowLog: ViewModifier {
    func body(content: Content) -> some View {
        content.onGeometryChange(for: CGRect?.self) { proxy in
            let frame = proxy.frame(in: .global)
            return frame.minX < -1 || frame.minY < -1 ? frame : nil
        } action: { frame in
            guard let frame else { return }
            Log.layout.fault("Window content is \(Int(frame.width)) by \(Int(frame.height)) points from \(Int(frame.minX)), \(Int(frame.minY)): it is larger than its window, so part of it is off-screen")
        }
    }
}
