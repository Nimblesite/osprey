typeDiagram
type FitParameter { node: Int key: String minimum: Float maximum: Float }
type FitConfig { parameters: List<FitParameter> seed: Int evaluations: Int tolerance: Float }
type FitCandidate { values: List<Float> loss: Float }
type FitSearch { best: FitCandidate step: Float seed: Float evaluations: Int history: List<Float> }
type FitResult { best: FitCandidate evaluations: Int history: List<Float> converged: Bool }
union ParameterScale { LinearScale {} LogarithmicScale {} }
union ModulationRule { Unmodulated {} Replacing { inlet: Int } Multiplying { inlet: Int } Exponential { inlet: Int octaves: Float } }
type Activity { active: Int capacity: Int stolen: Int energy: Float held: Bool }
