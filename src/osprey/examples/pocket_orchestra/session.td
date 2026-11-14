typeDiagram
union Instrument { Percussion {} Bass {} Chords {} }
union Phase { Begin {} Move {} End {} }
type Touch { finger: Int horizontal: Float vertical: Float }
type Player { peer: Int sequence: Int seen: Int instrument: Instrument touches: List<Touch> }
type Session { epoch: Int clock: Int players: List<Player> }
type Gesture { epoch: Int sequence: Int phase: Phase touch: Touch }
union Fault { InvalidPeer {} Full {} AlreadyJoined {} MissingPeer {} WrongEpoch {} StaleSequence {} InvalidTouch {} MissingTouch {} DuplicateTouch {} ClockReversed {} }
union Outcome<T> { Accepted { value: T } Rejected { fault: Fault } }
