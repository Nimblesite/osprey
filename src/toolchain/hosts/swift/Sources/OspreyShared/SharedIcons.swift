import SwiftUI

// Implements [UI-COMPONENTS-SYMBOLS]: a label's or press's hint names the icon it is led by, in
// the framework's symbol vocabulary. An unknown symbol draws a visible bordered square and adds
// a host diagnostic, never a silent omission.

/// Maps a framework symbol name to the system's own icon.
enum SharedIcons {
    private static let names: [String: String] = [
        "add": "plus",
        "delete": "trash",
        "dismiss": "xmark",
        "check": "checkmark",
        "warning": "exclamationmark.triangle",
        "info": "info.circle",
        "question": "questionmark.circle",
        "search": "magnifyingglass",
        "settings": "gearshape",
        "star": "star",
        "play": "play.fill",
        "pause": "pause.fill",
        "stop": "stop.fill",
        "refresh": "arrow.clockwise",
        "folder": "folder",
        "document": "doc",
        "package": "externaldrive",
        "inbox": "tray",
        "globe": "globe",
        "external": "arrow.up.forward.app",
        "lock": "eye.slash",
        "shield": "hand.raised",
        "branch": "point.3.connected.trianglepath.dotted",
        "tag": "asterisk",
        "clock": "clock",
        "person": "person",
        "cursor": "menubar.arrow.up.rectangle",
        "chevron": "chevron.right",
        "send": "arrow.up",
        "terminal": "apple.terminal",
        "edit": "square.and.pencil",
        "panel": "rectangle.sidebar.right",
        "sparkle": "sparkles",
    ]

    /// The icon a control's hint names, or a visible empty square the host can see is missing.
    @ViewBuilder
    static func symbol(_ name: String) -> some View {
        if let system = names[name] {
            Image(systemName: system).accessibilityHidden(true)
        } else {
            RoundedRectangle(cornerRadius: 2)
                .strokeBorder(.primary, lineWidth: 1)
                .frame(width: 16, height: 16)
                .accessibilityHidden(true)
                .help("no icon for symbol \(name)")
        }
    }

    /// Whether `name` spells a symbol this host knows.
    static func known(_ name: String) -> Bool {
        names[name] != nil
    }
}
