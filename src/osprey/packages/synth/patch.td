typeDiagram
type FitSource { identifier: String licence: String sampleRate: Int }
type FitReference { source: FitSource samples: List<Float> }
type FitReport<C,R> { algorithm: String algorithmLicence: String training: FitSource evaluation: FitSource config: C result: R evaluationLoss: Float }
