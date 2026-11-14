// Generated from typeDiagram [CONV-SWIFT].
import Foundation
public struct `VoiceControl` {
    public let `id`: String
    public let `peer`: Int64
    public let `instrument`: Int64
    public let `frequency`: Double
    public let `gain`: Double
    public let `tone`: Double
    public let `echo`: Double
    public let `reverb`: Double
    public let `warp`: Double
    public init(`id`: String, `peer`: Int64, `instrument`: Int64, `frequency`: Double, `gain`: Double, `tone`: Double, `echo`: Double, `reverb`: Double, `warp`: Double) {
        self.`id` = `id`
        self.`peer` = `peer`
        self.`instrument` = `instrument`
        self.`frequency` = `frequency`
        self.`gain` = `gain`
        self.`tone` = `tone`
        self.`echo` = `echo`
        self.`reverb` = `reverb`
        self.`warp` = `warp`
    }
}

public struct `VoiceState` {
    public let `control`: `VoiceControl`
    public let `bank`: Int64
    public let `target`: Double
    public let `gain`: Double
    public let `phase`: Double
    public let `tone`: Double
    public let `warp`: Double
    public init(`control`: `VoiceControl`, `bank`: Int64, `target`: Double, `gain`: Double, `phase`: Double, `tone`: Double, `warp`: Double) {
        self.`control` = `control`
        self.`bank` = `bank`
        self.`target` = `target`
        self.`gain` = `gain`
        self.`phase` = `phase`
        self.`tone` = `tone`
        self.`warp` = `warp`
    }
}

