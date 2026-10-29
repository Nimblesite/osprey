import SwiftUI

// Implements [UI-COMPONENTS]: symbol artwork is a host resource, never app logic.
private struct SymbolAssetsKey: EnvironmentKey {
    static let defaultValue: Result<SymbolAssets?, Error> = Result {
        guard let root = Bundle.main.resourceURL else { return nil }
        return try SymbolAssets.load(root: root)
    }
}

extension EnvironmentValues {
    var ospreySymbols: Result<SymbolAssets?, Error> {
        get { self[SymbolAssetsKey.self] }
        set { self[SymbolAssetsKey.self] = newValue }
    }
}

struct NativeSymbol: View {
    let name: String
    var size: CGFloat = 18
    @Environment(\.ospreySymbols) private var symbols

    @ViewBuilder var body: some View {
        switch symbols {
        case let .failure(error): failed(error.localizedDescription)
        case let .success(assets):
            if let image = assets?.image(name) {
                artwork(image)
            } else if let image = NSImage(systemSymbolName: name, accessibilityDescription: nil) {
                artwork(image)
            } else {
                failed("Unknown semantic symbol: \(name)")
            }
        }
    }

    private func artwork(_ image: NSImage) -> some View {
        Image(nsImage: image).renderingMode(.template).resizable().scaledToFit()
            .frame(width: size, height: size).accessibilityHidden(true)
    }

    private func failed(_ reason: String) -> some View {
        Image(systemName: "exclamationmark.triangle").frame(width: size, height: size)
            .accessibilityLabel("Icon unavailable: \(reason)").help(reason)
    }
}
