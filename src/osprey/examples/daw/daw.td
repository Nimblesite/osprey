typeDiagram
type Request { project: String folder: String }
type Written { parts: List<String> fault: String }
type Tone { breakpoint: Float feedback: Float attack: Float release: Float bloom: Float detune: Float }
type Hit { sweep: Float noise: Float colour: Float decay: Float }
union Follow { Constant {} Pitched {} Struck {} }
type Setting { key: String value: Float follow: Follow }
type PatchNode { identity: Int component: String size: Int seed: Int settings: List<Setting> }
type PatchCable { source: Int outlet: Int destination: Int inlet: Int }
type Routing { nodes: List<PatchNode> cables: List<PatchCable> attack: Float release: Float }
union Instrument { PatchInstrument { routing: Routing } PluckInstrument { brightness: Float decay: Float } BowInstrument { pressure: Float brightness: Float vibrato: Float } BellInstrument { decay: Float brightness: Float } MalletInstrument { decay: Float brightness: Float } OperatorInstrument { tone: Tone } DrumInstrument { hit: Hit } }
type Note { pitch: Int start: Float length: Float velocity: Float }
type Track { name: String instrument: Instrument gain: Float pan: Float reverb: Float notes: List<Note> }
type Space { size: Float decay: Float damping: Float }
type Afterglow { start: Float ratio: Float rise: Float hold: Float fade: Float gain: Float reverb: Float }
type Intro { recording: String tail: Float crossfade: Float }
type Arrangement { title: String tempo: Float sampleRate: Int drive: Float space: Space intro: Intro sustains: List<Afterglow> tracks: List<Track> }
type NoteRun<S> { held: S samples: List<Float> fault: String }
type NoteSpan { gate: Float total: Int }
type VoicePair<A, B> { one: A other: B }
type TrackMix { cursor: Int carry: List<Float> samples: List<Float> fault: String }
type Buses { left: List<Float> right: List<Float> wet: List<Float> }
type Mixdown { left: List<Float> right: List<Float> peak: Float }
