// Generated from typeDiagram [CONV-SWIFT].
import Foundation
public struct `VoiceControl` {
    public let `id`: String
    public let `frequency`: Double
    public let `gain`: Double
    public init(`id`: String, `frequency`: Double, `gain`: Double) {
        self.`id` = `id`
        self.`frequency` = `frequency`
        self.`gain` = `gain`
    }
}

public struct `VoiceState` {
    public let `id`: String
    public let `frequency`: Double
    public let `target`: Double
    public let `gain`: Double
    public let `phase`: Double
    public init(`id`: String, `frequency`: Double, `target`: Double, `gain`: Double, `phase`: Double) {
        self.`id` = `id`
        self.`frequency` = `frequency`
        self.`target` = `target`
        self.`gain` = `gain`
        self.`phase` = `phase`
    }
}

