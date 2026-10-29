import SwiftUI

// Implements [UI-COMPONENTS-FRAMED]: controls and backdrop effects inherit
// application-authored presentation values without application decisions.
struct FrameControls: ViewModifier {
    let style: FrameStyle
    let padding: Int?
    @Environment(\.ospreyControlHeight) private var height
    @Environment(\.ospreyQuietControls) private var quiet
    @Environment(\.ospreyTrailingSymbols) private var trailing

    func body(content: Content) -> some View {
        content.environment(\.ospreyControlHeight, (style.controlHeight ?? 0) > 0 ? style.controlHeight ?? height : height)
            .environment(\.ospreyControlPadding, (style.controlPadding ?? -1) >= 0 ? style.controlPadding : padding)
            .environment(\.ospreyQuietControls, style.quietControls == true || quiet)
            .environment(\.ospreyIconOnly, style.iconOnly ?? false)
            .environment(\.ospreyFlatRows, style.flatRows ?? false)
            .environment(\.ospreyFadeBackdrop, style.fadeBackdrop ?? false)
            .environment(\.ospreyTrailingSymbols, style.trailingSymbols == true || trailing)
    }
}

private struct FadeBackdropKey: EnvironmentKey { static let defaultValue = false }
private struct TrailingSymbolsKey: EnvironmentKey { static let defaultValue = false }

extension EnvironmentValues {
    var ospreyFadeBackdrop: Bool {
        get { self[FadeBackdropKey.self] }
        set { self[FadeBackdropKey.self] = newValue }
    }

    var ospreyTrailingSymbols: Bool {
        get { self[TrailingSymbolsKey.self] }
        set { self[TrailingSymbolsKey.self] = newValue }
    }
}
