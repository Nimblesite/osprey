import CoreText
import Foundation

// Implements [WORKBENCH-DESIGN-TYPE] for every app on this host: typefaces travel with the
// application and are registered process-locally, so nothing is installed on the person's Mac.
// An application that bundles no fonts simply keeps the system faces.

/// Registers the fonts an application bundled, once per process.
public enum FontRegistration {
    /// The serif text face the Workbench bundles.
    public static let serifName = "STIXTwoText"
    /// The italic serif text face.
    public static let serifItalicName = "STIXTwoText-Italic"
    /// The mathematics face glyphs align on.
    public static let mathName = "STIXTwoMath-Regular"

    /// The faces an application may bundle, in its resources' `fonts` folder.
    private static let faces = ["STIXTwoText.ttf", "STIXTwoText-Italic.ttf", "STIXTwoMath-Regular.ttf", "Geist.ttf"]

    /// Whether an authored line height sets every line's box exactly, as the browser's does
    /// ([WORKBENCH-DESIGN-TYPE]): only an application that bundles its faces asks for it, so every
    /// other application keeps the platform's own line boxes.
    public internal(set) static var exactLines = false

    /// Registered before AppKit initializes text rendering; retains explicit user preferences.
    /// macOS 26 rasterizes text heavier than earlier releases: only there does turning smoothing
    /// off match the browser's regular stroke. Only an application that bundles these faces may
    /// set it, so every other application's text renders exactly as before.
    public static func configureTextRendering() {
        guard bundled() else { return }
        exactLines = true
        if #available(macOS 26, *) {
            UserDefaults.standard.register(defaults: ["AppleFontSmoothing": 0])
        }
    }

    /// Registers every bundled face that is present; an application that ships none is no error.
    public static func registerBundled() {
        guard let root = Bundle.main.resourceURL else { return }
        faces.forEach { name in
            let url = root.appendingPathComponent("fonts").appendingPathComponent(name)
            guard FileManager.default.fileExists(atPath: url.path) else { return }
            var error: Unmanaged<CFError>?
            _ = CTFontManagerRegisterFontsForURL(url as CFURL, .process, &error)
        }
    }

    /// Whether the application bundled any of the faces this host registers.
    private static func bundled() -> Bool {
        guard let root = Bundle.main.resourceURL else { return false }
        return faces.contains { name in
            FileManager.default.fileExists(atPath: root.appendingPathComponent("fonts").appendingPathComponent(name).path)
        }
    }
}
