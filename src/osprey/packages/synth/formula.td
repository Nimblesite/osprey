typeDiagram
union FormulaOperator { Plus {} Minus {} Times {} Over {} Modulo {} Power {} }
union FormulaFunction { SineOf {} CosineOf {} MagnitudeOf {} FloorOf {} RoundOf {} RootOf {} }
union FormulaTerm { Literal { value: Float } StepIndex {} Negated { operand: FormulaTerm } Binary { operation: FormulaOperator left: FormulaTerm right: FormulaTerm } Call { applied: FormulaFunction argument: FormulaTerm } }
union FormulaToken { NumberToken { value: Float } WordToken { text: String } SymbolToken { text: String } }
type Lexeme { token: FormulaToken column: Int }
type FormulaParse { term: FormulaTerm rest: List<Lexeme> }
type OperatorSpelling { symbol: String operation: FormulaOperator }
union FormulaNote { Rest {} Sounded { pitch: Int } }
