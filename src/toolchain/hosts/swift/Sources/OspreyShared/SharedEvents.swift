import Foundation

// Implements [NATIVE-SHM-EVENT]: host events as the typed call the application exports — a
// kind, the control it names, two texts, two numbers and two flags. Nothing is encoded; what a
// control means is the application's, through the binding its number names.

/// One host event, as `osprey_app_event` takes it after the host clock.
public struct Touch: Sendable, Equatable {
    public let kind: Int64
    public let ordinal: Int64
    public let words: String
    public let detail: String
    public let x: Double
    public let y: Double
    public let down: Bool
    public let shift: Bool

    public init(kind: Int64, ordinal: Int64 = 0, words: String = "", detail: String = "",
                x: Double = 0, y: Double = 0, down: Bool = false, shift: Bool = false) {
        self.kind = kind
        self.ordinal = ordinal
        self.words = words
        self.detail = detail
        self.x = x
        self.y = y
        self.down = down
        self.shift = shift
    }
}

/// The event kinds the application's `ui::Hosting` declares.
public enum TouchKind {
    public static let control: Int64 = 0
    public static let key: Int64 = 1
    public static let pointer: Int64 = 2
    public static let viewed: Int64 = 3
    public static let point: Int64 = 4
    public static let stroke: Int64 = 5
    public static let enclose: Int64 = 6
    public static let lasso: Int64 = 7
    public static let completed: Int64 = 8
    public static let phase: Int64 = 9
    public static let frame: Int64 = 10
    public static let appearance: Int64 = 11
}

extension Touch {
    /// The control numbered `ordinal` delivering `words`: empty for a tap, `true`/`false` for a
    /// switch, a key for a picker, text for a field, a whole number for a splitter or caret.
    public static func control(_ ordinal: Int64, _ words: String) -> Touch {
        Touch(kind: TouchKind.control, ordinal: ordinal, words: words)
    }

    /// A key going down or up over the scene numbered `ordinal`.
    public static func key(_ ordinal: Int64, _ name: String, _ down: Bool) -> Touch {
        Touch(kind: TouchKind.key, ordinal: ordinal, words: name, down: down)
    }

    /// The pointer over the scene numbered `ordinal`, in scene units.
    public static func pointer(_ ordinal: Int64, x: Double, y: Double, down: Bool, shift: Bool) -> Touch {
        Touch(kind: TouchKind.pointer, ordinal: ordinal, x: x, y: y, down: down, shift: shift)
    }

    /// The scene numbered `ordinal` measured in scene units.
    public static func viewed(_ ordinal: Int64, width: Double, height: Double) -> Touch {
        Touch(kind: TouchKind.viewed, ordinal: ordinal, x: width, y: height)
    }

    /// The capability under `ticket` finished: its result text when it succeeded, or the host's
    /// reason when it failed.
    public static func completed(_ ticket: String, result: String) -> Touch {
        Touch(kind: TouchKind.completed, words: ticket, detail: result, down: true)
    }

    /// The capability under `ticket` failed, for `reason`.
    public static func completed(_ ticket: String, failed reason: String) -> Touch {
        Touch(kind: TouchKind.completed, words: ticket, detail: reason, down: false)
    }

    /// A lifecycle phase, such as `active`, `closed` or `open:<path>`.
    public static func phase(_ name: String) -> Touch {
        Touch(kind: TouchKind.phase, words: name)
    }

    /// One display frame, `elapsed` seconds after the last.
    public static func frame(_ elapsed: Double) -> Touch {
        Touch(kind: TouchKind.frame, x: elapsed)
    }

    /// The system appearance changed.
    public static func appearance(_ dark: Bool) -> Touch {
        Touch(kind: TouchKind.appearance, down: dark)
    }
}
