import CoreText
import Foundation

// Implements [WORKBENCH-DESIGN-TYPE]: mathematical faces travel with the app.
// Registration is process-local and never installs fonts on the user's Mac.
public enum FontRegistration {
    public static let serifName = "STIXTwoText"
    public enum Failure: Error {
        case missing(String)
        case refused(String)
    }

    // Register before AppKit initializes text rendering; retain explicit user preferences.
    public static func configureTextRendering() {
        UserDefaults.standard.register(defaults: ["AppleFontSmoothing": 0])
    }

    public static func registerBundled() throws {
        for name in ["STIXTwoText.ttf", "STIXTwoText-Italic.ttf", "STIXTwoMath-Regular.ttf", "Geist.ttf"] {
            try register(name)
        }
    }

    private static func register(_ name: String) throws {
        guard let root = Bundle.main.resourceURL else { throw Failure.missing(name) }
        let url = root.appendingPathComponent("fonts").appendingPathComponent(name)
        guard FileManager.default.fileExists(atPath: url.path) else { throw Failure.missing(name) }
        var error: Unmanaged<CFError>?
        guard CTFontManagerRegisterFontsForURL(url as CFURL, .process, &error) else {
            throw Failure.refused(error?.takeRetainedValue().localizedDescription ?? name)
        }
    }
}
